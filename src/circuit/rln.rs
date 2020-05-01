use crate::circuit::polynomial::allocate_add_with_coeff;
use crate::circuit::poseidon::PoseidonCircuit;
use crate::poseidon::{Poseidon as PoseidonHasher, PoseidonParams};
use sapling_crypto::bellman::pairing::Engine;
use sapling_crypto::bellman::{Circuit, ConstraintSystem, SynthesisError, Variable};
use sapling_crypto::circuit::{boolean, ecc, num, Assignment};
use sapling_crypto::jubjub::{JubjubEngine, JubjubParams, PrimeOrder};

// Rate Limit Nullifier

#[derive(Clone)]
pub struct RLNInputs<E>
where
  E: Engine,
{
  // Public inputs

  // share, (x, y),
  // where x should be hash of the signal
  // and y is the evaluation
  pub share_x: Option<E::Fr>,
  pub share_y: Option<E::Fr>,

  // epoch is the external nullifier
  // we derive the line equation and the nullifier from epoch
  pub epoch: Option<E::Fr>,

  // nullifier
  pub nullifier: Option<E::Fr>,

  // root is the current state of membership set
  pub root: Option<E::Fr>,

  // Private inputs

  // id_key must be a preimage of a leaf in membership tree.
  // id_key also together with epoch will be used to construct
  // a secret line equation together with the epoch
  pub id_key: Option<E::Fr>,

  // authentication path of the member
  pub auth_path: Vec<Option<(E::Fr, bool)>>,
}

impl<E> RLNInputs<E>
where
  E: Engine,
{
  fn public_inputs(self) -> Vec<E::Fr> {
    vec![
      self.root.unwrap(),
      self.epoch.unwrap(),
      self.share_x.unwrap(),
      self.share_y.unwrap(),
      self.nullifier.unwrap(),
    ]
  }
}

#[derive(Clone)]
pub struct RLNCircuit<E>
where
  E: Engine,
{
  pub inputs: RLNInputs<E>,
  pub hasher: PoseidonCircuit<E>,
}

impl<E> Circuit<E> for RLNCircuit<E>
where
  E: Engine,
{
  fn synthesize<CS: ConstraintSystem<E>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
    // 1. Part
    // Membership constraints
    // root == merkle_proof(auth_path, preimage_of_leaf)

    let root = num::AllocatedNum::alloc(cs.namespace(|| "root"), || {
      let value = self.inputs.root.clone();
      Ok(*value.get()?)
    })?;
    root.inputize(cs.namespace(|| "root is public"))?;

    let preimage = num::AllocatedNum::alloc(cs.namespace(|| "preimage"), || {
      let value = self.inputs.id_key;
      Ok(*value.get()?)
    })?;

    // identity is a leaf of membership tree

    let identity = self.hasher.alloc(cs.namespace(|| "identity"), vec![preimage.clone()])?;

    // accumulator up to the root

    let mut acc = identity.clone();

    // ascend the tree

    let auth_path_witness = self.inputs.auth_path.clone();
    for (i, e) in auth_path_witness.into_iter().enumerate() {
      let cs = &mut cs.namespace(|| format!("auth path {}", i));
      let position = boolean::Boolean::from(boolean::AllocatedBit::alloc(
        cs.namespace(|| "position bit"),
        e.map(|e| e.1),
      )?);
      let path_element = num::AllocatedNum::alloc(cs.namespace(|| "path element"), || Ok(e.get()?.0))?;

      let (xr, xl) = num::AllocatedNum::conditionally_reverse(
        cs.namespace(|| "conditional reversal of preimage"),
        &acc,
        &path_element,
        &position,
      )?;

      acc = self.hasher.alloc(cs.namespace(|| "hash couple"), vec![xl, xr])?;
    }

    // see if it is a member

    cs.enforce(
      || "enforce membership",
      |lc| lc + acc.get_variable(),
      |lc| lc + CS::one(),
      |lc| lc + root.get_variable(),
    );

    // 2. Part
    // Line Equation Constaints
    // a_1 = hash(a_0, epoch)
    // share_y == a_0 + a_1 * share_x

    let epoch = num::AllocatedNum::alloc(cs.namespace(|| "epoch"), || {
      let value = self.inputs.epoch.clone();
      Ok(*value.get()?)
    })?;
    epoch.inputize(cs.namespace(|| "epoch is public"))?;

    let a_0 = preimage.clone();

    // a_1 == h(a_0, epoch)

    let a_1 = self.hasher.alloc(cs.namespace(|| "a_1"), vec![a_0.clone(), epoch])?;

    let share_x = num::AllocatedNum::alloc(cs.namespace(|| "share x"), || {
      let value = self.inputs.share_x.clone();
      Ok(*value.get()?)
    })?;
    share_x.inputize(cs.namespace(|| "share x is public"))?;

    // constaint the evaluation the line equation

    let eval = allocate_add_with_coeff(cs.namespace(|| "eval"), &a_1, &share_x, &a_0)?;

    let share_y = num::AllocatedNum::alloc(cs.namespace(|| "share y"), || {
      let value = self.inputs.share_y.clone();
      Ok(*value.get()?)
    })?;
    share_y.inputize(cs.namespace(|| "share y is public"))?;

    // see if share satisfies the line equation

    cs.enforce(
      || "enforce lookup",
      |lc| lc + share_y.get_variable(),
      |lc| lc + CS::one(),
      |lc| lc + eval.get_variable(),
    );

    // 3. Part
    // Nullifier constraints

    // hashing secret twice with epoch ingredient
    // a_1 == hash(a_0, epoch) is already constrained

    // nullifier == hash(a_1)

    let nullifier_calculated = self
      .hasher
      .alloc(cs.namespace(|| "calculated nullifier"), vec![a_1.clone()])?;

    let nullifier = num::AllocatedNum::alloc(cs.namespace(|| "nullifier"), || {
      let value = self.inputs.nullifier.clone();
      Ok(*value.get()?)
    })?;
    nullifier.inputize(cs.namespace(|| "nullifier is public"))?;

    // check if correct nullifier supplied

    cs.enforce(
      || "enforce nullifier",
      |lc| lc + nullifier_calculated.get_variable(),
      |lc| lc + CS::one(),
      |lc| lc + nullifier.get_variable(),
    );

    Ok(())
  }
}

#[cfg(test)]
mod test {
  use super::{RLNCircuit, RLNInputs};
  use crate::circuit::poseidon::PoseidonCircuit;
  use crate::merkle::MerkleTree;
  use crate::poseidon::{Poseidon as PoseidonHasher, PoseidonParams};
  use rand::{Rand, SeedableRng, XorShiftRng};
  use sapling_crypto::bellman::groth16::{
    create_random_proof, generate_random_parameters, prepare_verifying_key, verify_proof,
  };
  use sapling_crypto::bellman::pairing::ff::{Field, PrimeField, PrimeFieldRepr};
  use sapling_crypto::bellman::pairing::Engine;
  use sapling_crypto::bellman::Circuit;
  use sapling_crypto::circuit::test::TestConstraintSystem;
  use std::thread::sleep;
  use std::time::{Duration, Instant};

  struct RLNTest<E>
  where
    E: Engine,
  {
    // cs: TestConstraintSystem<E>,
    merkle_depth: usize,
    poseidon_params: PoseidonParams<E>,
  }

  impl<E> RLNTest<E>
  where
    E: Engine,
  {
    pub fn new(poseidon_params: PoseidonParams<E>, merkle_depth: usize) -> RLNTest<E> {
      // let cs = TestConstraintSystem::<E>::new();
      RLNTest::<E> {
        poseidon_params,
        merkle_depth,
      }
    }

    fn inputs(&self) -> RLNInputs<E> {
      let mut rng = XorShiftRng::from_seed([0x3dbe6258, 0x8d313d76, 0x3237db17, 0xe5bc0654]);
      let mut hasher = PoseidonHasher::new(self.poseidon_params.clone());
      // Initialize empty merkle tree
      let merkle_depth = self.merkle_depth;
      let mut membership_tree = MerkleTree::empty(hasher.clone(), merkle_depth);

      // A. setup an identity

      let id_key = E::Fr::rand(&mut rng);
      let id_comm = hasher.hash(vec![id_key.clone()]);

      // B. insert to the membership tree

      let id_index = 6; // any number below 2^depth will work
      membership_tree.update(id_index, id_comm);

      // C.1 get membership witness

      let auth_path = membership_tree.witness(id_index);
      assert!(membership_tree.check_inclusion(auth_path.clone(), id_index, id_key.clone()));

      // C.2 prepare sss

      // get current epoch
      let epoch = E::Fr::rand(&mut rng);

      let signal_hash = E::Fr::rand(&mut rng);
      // evaluation point is the signal_hash
      let share_x = signal_hash.clone();

      // calculate current line equation
      let a_0 = id_key.clone();
      let a_1 = hasher.hash(vec![a_0, epoch]);

      // evaluate line equation
      let mut share_y = a_1.clone();
      share_y.mul_assign(&share_x);
      share_y.add_assign(&a_0);

      // calculate nullfier
      let nullifier = hasher.hash(vec![a_1]);

      // compose the circuit

      let inputs = RLNInputs::<E> {
        share_x: Some(share_x),
        share_y: Some(share_y),
        epoch: Some(epoch),
        nullifier: Some(nullifier),
        root: Some(membership_tree.root()),
        id_key: Some(id_key),
        auth_path: auth_path.into_iter().map(|w| Some(w)).collect(),
      };

      inputs
    }

    fn empty_inputs(&self) -> RLNInputs<E> {
      RLNInputs::<E> {
        share_x: None,
        share_y: None,
        epoch: None,
        nullifier: None,
        root: None,
        id_key: None,
        auth_path: vec![None; self.merkle_depth],
      }
    }

    pub fn run(&self) {
      let mut rng = XorShiftRng::from_seed([0x3dbe6258, 0x8d313d76, 0x3237db17, 0xe5bc0654]);

      let hasher = PoseidonCircuit::new(self.poseidon_params.clone());
      let inputs = self.inputs();
      let circuit = RLNCircuit::<E> {
        inputs: inputs.clone(),
        hasher: hasher.clone(),
      };
      let mut cs = TestConstraintSystem::<E>::new();
      {
        let circuit = circuit.clone();
        circuit.synthesize(&mut cs).unwrap();
        let unsatisfied = cs.which_is_unsatisfied();
        if unsatisfied.is_some() {
          panic!("unsatisfied\n{}", unsatisfied.unwrap());
        }
        let unconstrained = cs.find_unconstrained();
        if !unconstrained.is_empty() {
          panic!("unconstrained\n{}", unconstrained);
        }
        assert!(cs.is_satisfied());
        println!("number of constaints: {}", cs.num_constraints());
      }

      {
        let parameters = {
          let inputs = self.empty_inputs();
          let circuit = RLNCircuit::<E> {
            inputs,
            hasher: hasher.clone(),
          };
          let parameters = generate_random_parameters(circuit, &mut rng).unwrap();
          parameters
        };

        let mut key_size = 0;
        let point_size = (((E::Fr::NUM_BITS / 64) + 1) * 8 * 2) as usize;
        key_size += parameters.a.len() * point_size;
        key_size += parameters.l.len() * point_size;
        key_size += parameters.h.len() * point_size;
        key_size += parameters.b_g1.len() * point_size;
        key_size += parameters.b_g2.len() * point_size * 2;

        println!("prover key size in bytes: {}", key_size);

        let now = Instant::now();
        let proof = create_random_proof(circuit, &parameters, &mut rng).unwrap();
        println!("prover time: {}", now.elapsed().as_millis() as f64 / 1000.0);

        let verifing_key = prepare_verifying_key(&parameters.vk);
        assert!(verify_proof(&verifing_key, &proof, &inputs.public_inputs()).unwrap());
      }
    }
  }

  #[test]
  fn test_rln() {
    use sapling_crypto::bellman::pairing::bn256::Bn256;
    let poseidon_params = PoseidonParams::<Bn256>::default();
    let rln_test = RLNTest::new(poseidon_params, 32);
    rln_test.run();
  }
}