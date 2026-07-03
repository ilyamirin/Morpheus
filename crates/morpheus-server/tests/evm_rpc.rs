use morpheus_server::evm_rpc::{parse_hex_quantity, rpc_log_from_value, rpc_receipt_from_value};
use serde_json::json;

#[test]
fn parses_hex_quantities_strictly() {
    assert_eq!(parse_hex_quantity("0x0").unwrap(), 0);
    assert_eq!(parse_hex_quantity("0x2a").unwrap(), 42);
    assert!(parse_hex_quantity("42").is_err());
    assert!(parse_hex_quantity("0x").is_err());
    assert!(parse_hex_quantity("0xzz").is_err());
}

#[test]
fn parses_rpc_log_with_required_fields() {
    let log = rpc_log_from_value(json!({
        "address": "0x0000000000000000000000000000000000000001",
        "blockHash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "blockNumber": "0x10",
        "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "logIndex": "0x2",
        "topics": [
            "0x1111111111111111111111111111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222222222222222222222222222"
        ],
        "data": "0x"
    }))
    .unwrap();

    assert_eq!(log.block_number, 16);
    assert_eq!(log.log_index, 2);
    assert_eq!(log.topics.len(), 2);
}

#[test]
fn parses_successful_receipt_status() {
    let receipt = rpc_receipt_from_value(json!({
        "transactionHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "blockHash": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "blockNumber": "0x10",
        "status": "0x1"
    }))
    .unwrap();

    assert!(receipt.success);
    assert_eq!(receipt.block_number, 16);
}
