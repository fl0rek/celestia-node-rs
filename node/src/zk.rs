use sp1_primitives::io::SP1PublicValues;
use serde::{Serialize, Deserialize};
//use sp1_sdk::{SP1ProofWithPublicValues,SP1VerificationError};

use celestia_types::ExtendedHeader;
use celestia_tendermint::hash::{Hash, Algorithm};

use sp1_verifier::{Groth16Verifier, Groth16Error};

use crate::p2p;

//const ELF: &[u8] = include_bytes!("../../riscv32im-succinct-zkvm-elf");
const VK: &str = "0x001cbf33fdd2568c6a6a318814dad8b797a7e7e4987cd2a4aeca95e06cf55bfc";
const GROTH16_VK: &[u8] = include_bytes!("../../groth16_vk.bin");
const PROOF_URL: &str = "http://localhost:9000/latest_proof.json";

#[derive(thiserror::Error, Debug)]
pub enum ZkError {
    #[error("Could not fetch proof")]
    ProofFetchError(#[from] reqwest::Error),
    #[error("Could not fetch proven header")]
    HeaderFetchError(#[from] p2p::P2pError),
    #[error("Invalid Public values")]
    InvalidPublicValues,
    #[error("Could not verify proof")]
    ProofVerificationError(#[from] Groth16Error),
    #[error("Could not verify downloaded header")]
    HeaderVerificationError(celestia_types::Error)
}

pub struct ProofPublicCommitments {
    vk_hash: Vec<u8>,
    genesis_hash: Hash,
    header_hash: Hash,
}

#[derive(Serialize, Deserialize)]
pub struct WebFriendlyProof {
    pub proof: Vec<u8>,
    pub public_values: SP1PublicValues
}

impl TryFrom<SP1PublicValues> for ProofPublicCommitments {
    type Error = ZkError;

    fn try_from(mut item: SP1PublicValues) -> Result<ProofPublicCommitments, Self::Error> {
        let vk_hash = item.read();
        let genesis_hash : Vec<u8> = item.read();
        let header_hash : Vec<u8> = item.read();
        let zk_program_result : bool = item.read();

        if !zk_program_result {
            return Err(ZkError::InvalidPublicValues);
        }

        Ok(ProofPublicCommitments {
            vk_hash,
            genesis_hash: Hash::from_bytes(Algorithm::Sha256, &genesis_hash)
                .map_err(|_| ZkError::InvalidPublicValues) ?,
            header_hash: Hash::from_bytes(Algorithm::Sha256, &genesis_hash)
                .map_err(|_| ZkError::InvalidPublicValues) ?,
        })
    }
}

pub async fn get_verified_network_head(p2p: &p2p::P2p) -> Result<ExtendedHeader, ZkError> {
    let serialised_proof = reqwest::get(PROOF_URL).await?.text().await?;
    /*
    let proof_with_public_values: SP1ProofWithPublicValues =
        serde_json::from_str(&serialised_proof).expect("could not parse proof");
    */
    let proof_with_public_values : WebFriendlyProof = serde_json::from_str(&serialised_proof).expect("could not parse");

    // TODO: lol, this can panic on invalid proof
    //let proof = proof_with_public_values.bytes();
    let proof = proof_with_public_values.proof;
    //println!("{proof:?}");
    let public_inputs = proof_with_public_values.public_values.to_vec();
    let public_commitments = ProofPublicCommitments::try_from(proof_with_public_values.public_values.clone())?;

    //let groth16_vk_hash: [u8; 4] = Sha256::digest(groth16_vk)[..4].try_into().unwrap();
    //tracing::info!("{:?}", &groth16_vk_hash);

    tracing::info!("{:?}", &proof[..4]);

    let verifier = Groth16Verifier::verify(&proof, &public_inputs, VK, &GROTH16_VK)?;

    // TODO: this sometimes fails (esp when testing on genesis header..), retries to avoid spurious failures?
    // there are retries in `try_init`, but that would cause proof to be re-validated
    let proven_header = p2p.get_header(public_commitments.header_hash).await?;

    // TODO: do we need to call validate & verify? 
    proven_header.validate().map_err(ZkError::HeaderVerificationError)?;

    Ok(proven_header)
}
