use pre_block::fixture::NarwhalFixture;
use tezos_smart_rollup_encoding::{inbox::ExternalMessageFrame, smart_rollup::SmartRollupAddress};
use tezos_smart_rollup_mock::MockHost;

use crate::{
    kernel_loop,
    storage::{read_head, write_authorities},
};

#[test]
fn test_external_message() {
    let mut mock_host = MockHost::default();

    let mut narwhal_fixture = NarwhalFixture::default();
    let pre_block = narwhal_fixture.next_pre_block(1);

    // Initialize authorities

    write_authorities(&mut mock_host, 0, &narwhal_fixture.authorities());

    // Send external message

    let mut contents = Vec::with_capacity(2048);
    contents.push(1);
    contents.extend_from_slice(bcs::to_bytes(&pre_block).unwrap().as_slice());
    assert!(contents.len() < 2048);

    let message = ExternalMessageFrame::Targetted {
        address: SmartRollupAddress::from_b58check("sr163Lv22CdE8QagCwf48PWDTquk6isQwv57").unwrap(),
        contents,
    };

    mock_host.add_external(message);

    kernel_loop(&mut mock_host);

    let head = read_head(&mock_host);
    assert_eq!(1, head);
}
