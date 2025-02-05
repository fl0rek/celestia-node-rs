use sp1_sdk::SP1ProofWithPublicValues;
use sp1_primitives::io::SP1PublicValues;
use serde::{Serialize, Deserialize};

use lumina_node::zk::WebFriendlyProof;

/*
#[derive(Serialize, Deserialize)]
struct WebFriendlyProof {
    proof: Vec<u8>,
    public_values: SP1PublicValues
}
*/

// serve with:
// nix run nixpkgs\#python3 -- -m http.server 9000

// convert sp1-sdk SP1ProofWithPublicValues type to our web-friendly one
// (sp1-sdk >4.0.0 crate doesn't compile for wasm)
fn main() {
    let proof_with_public_values: SP1ProofWithPublicValues =
        serde_json::from_reader(std::io::stdin()).expect("could not parse proof");

    let web = WebFriendlyProof {
        proof: proof_with_public_values.bytes(),
        public_values: proof_with_public_values.public_values,
    };

    serde_json::to_writer_pretty(std::io::stdout(), &web).expect("could not serialize proof");
}
