use hex;
use sapling_crypto::bellman::pairing::Engine;
use std::fs::File;
use std::io::prelude::*;

fn main() {
  use sapling_crypto::bellman::pairing::bn256::Bn256;
  let merkle_depth = 32usize;
  export_test_keys::<Bn256>(merkle_depth);
}

fn export_test_keys<E: Engine>(merkle_depth: usize) {
  // use rand::{Rand, SeedableRng, XorShiftRng};
  // use rln::circuit::poseidon::PoseidonCircuit;
  // use rln::circuit::rln::{RLNCircuit, RLNInputs};
  // use rln::poseidon::PoseidonParams;
  use sapling_crypto::bellman::groth16::{generate_random_parameters, Parameters};

  // let poseidon_params = PoseidonParams::<E>::default();
  // let mut rng = XorShiftRng::from_seed([0x3dbe6258, 0x8d313d76, 0x3237db17, 0xe5bc0654]);
  // let hasher = PoseidonCircuit::new(poseidon_params.clone());
  // let circuit = RLNCircuit::<E> {
  //   inputs: RLNInputs::<E>::empty(merkle_depth),
  //   hasher: hasher.clone(),
  // };
  // let parameters = generate_random_parameters(circuit, &mut rng).unwrap();
  // let mut file_vk = File::create("verifier.key").unwrap();
  // let vk = parameters.vk.clone();

  // let s = "1122";
  // let r = hex::decode(s);
  // println!("{:?}", r);

  let mut s: Vec<u8> = vec![0; 63];
  // vk.write(&mut s).unwrap();
  // println!("{:x?}", s);
  // println!("{}", s.len());
  // println!("{}", hex::encode(&s));
  // file_vk.write_all(&s).unwrap();

  let parameters2 = Parameters::<E>::read(&mut s.as_slice(), true).unwrap();
  println!("{:?}", parameters2.h);
  println!("xxx");
}
