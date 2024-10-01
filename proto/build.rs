//! A build script generating rust types from protobuf definitions.

use anyhow::Result;

const SERIALIZED: &str = r#"#[derive(::serde::Deserialize, ::serde::Serialize)]"#;
const SERIALIZED_DEFAULT: &str =
    r#"#[derive(::serde::Deserialize, ::serde::Serialize)] #[serde(default)]"#;
const TRANSPARENT: &str = r#"#[serde(transparent)]"#;
const BASE64STRING: &str =
    r#"#[serde(with = "celestia_tendermint_proto::serializers::bytes::base64string")]"#;
const QUOTED: &str = r#"#[serde(with = "celestia_tendermint_proto::serializers::from_str")]"#;
const VEC_BASE64STRING: &str =
    r#"#[serde(with = "celestia_tendermint_proto::serializers::bytes::vec_base64string")]"#;
const OPTION_ANY: &str = r#"#[serde(with = "crate::serializers::option_any")]"#;
const OPTION_TIMESTAMP: &str = r#"#[serde(with = "crate::serializers::option_timestamp")]"#;
const NULL_DEFAULT: &str = r#"#[serde(with = "crate::serializers::null_default")]"#;

#[rustfmt::skip]
static CUSTOM_TYPE_ATTRIBUTES: &[(&str, &str)] = &[
    (".celestia.da.DataAvailabilityHeader", SERIALIZED_DEFAULT),
    (".celestia.blob.v1.MsgPayForBlobs", SERIALIZED_DEFAULT),
    (".cosmos.base.abci.v1beta1.ABCIMessageLog", SERIALIZED_DEFAULT),
    (".cosmos.base.abci.v1beta1.Attribute", SERIALIZED_DEFAULT),
    (".cosmos.base.abci.v1beta1.StringEvent", SERIALIZED_DEFAULT),
    (".cosmos.base.abci.v1beta1.TxResponse", SERIALIZED_DEFAULT),
    (".cosmos.base.v1beta1.Coin", SERIALIZED_DEFAULT),
    (".cosmos.base.query.v1beta1.PageResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.QueryDelegationResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.DelegationResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.Delegation", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.QueryRedelegationsResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.RedelegationResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.Redelegation", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.RedelegationEntryResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.RedelegationEntry", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.QueryUnbondingDelegationResponse", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.UnbondingDelegation", SERIALIZED_DEFAULT),
    (".cosmos.staking.v1beta1.UnbondingDelegationEntry", SERIALIZED_DEFAULT),
    (".header.pb.ExtendedHeader", SERIALIZED_DEFAULT),
    (".share.eds.byzantine.pb.BadEncoding", SERIALIZED_DEFAULT),
    (".share.eds.byzantine.pb.Share", SERIALIZED_DEFAULT),
    (".proof.pb.Proof", SERIALIZED_DEFAULT),
    (".shwap.AxisType", SERIALIZED),
    (".shwap.Row", SERIALIZED),
    (".shwap.RowNamespaceData", SERIALIZED_DEFAULT),
    (".shwap.Sample", SERIALIZED_DEFAULT),
    (".shwap.Share", SERIALIZED_DEFAULT),
    (".shwap.Share", TRANSPARENT),
];

#[rustfmt::skip]
static CUSTOM_FIELD_ATTRIBUTES: &[(&str, &str)] = &[
    (".celestia.da.DataAvailabilityHeader.row_roots", VEC_BASE64STRING),
    (".celestia.da.DataAvailabilityHeader.column_roots", VEC_BASE64STRING),
    (".cosmos.base.abci.v1beta1.TxResponse.tx", OPTION_ANY),
    (".cosmos.base.query.v1beta1.PageResponse.next_key", BASE64STRING),
    (".cosmos.staking.v1beta1.RedelegationEntry.completion_time", OPTION_TIMESTAMP),
    (".cosmos.staking.v1beta1.UnbondingDelegationEntry.completion_time", OPTION_TIMESTAMP),
    (".share.eds.byzantine.pb.BadEncoding.axis", QUOTED),
    (".proof.pb.Proof.nodes", VEC_BASE64STRING),
    (".proof.pb.Proof.leaf_hash", BASE64STRING),
    (".shwap.RowNamespaceData.shares", NULL_DEFAULT),
    (".shwap.Share", BASE64STRING),
];

fn main() -> Result<()> {
    let mut config = prost_build::Config::new();

    for (type_path, attr) in CUSTOM_TYPE_ATTRIBUTES {
        config.type_attribute(type_path, attr);
    }

    for (field_path, attr) in CUSTOM_FIELD_ATTRIBUTES {
        config.field_attribute(field_path, attr);
    }

    config
        .include_file("mod.rs")
        .extern_path(".tendermint", "::celestia_tendermint_proto::v0_34")
        .extern_path(
            ".google.protobuf.Timestamp",
            "::celestia_tendermint_proto::google::protobuf::Timestamp",
        )
        .extern_path(
            ".google.protobuf.Duration",
            "::celestia_tendermint_proto::google::protobuf::Duration",
        )
        // Comments in Google's protobuf are causing issues with cargo-test
        .disable_comments([".google"])
        .compile_protos(
            &[
                "vendor/celestia/da/data_availability_header.proto",
                "vendor/celestia/blob/v1/tx.proto",
                "vendor/header/pb/extended_header.proto",
                "vendor/share/eds/byzantine/pb/share.proto",
                "vendor/share/shwap/pb/shwap.proto",
                "vendor/share/shwap/p2p/bitswap/pb/bitswap.proto",
                "vendor/cosmos/base/v1beta1/coin.proto",
                "vendor/cosmos/base/abci/v1beta1/abci.proto",
                "vendor/cosmos/crypto/multisig/v1beta1/multisig.proto",
                "vendor/cosmos/staking/v1beta1/query.proto",
                "vendor/cosmos/tx/v1beta1/tx.proto",
                "vendor/go-header/p2p/pb/header_request.proto",
            ],
            &["vendor", "vendor/nmt"],
        )?;

    Ok(())
}
