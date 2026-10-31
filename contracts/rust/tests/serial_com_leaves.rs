// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! PLUGIN.SERIAL_COM's leaves are declared, with the retain class its source uses.

use apkaudio_contracts::baremetal_io::{module_incoming, module_outgoing, module_port};

#[test]
fn serial_com_leaves() {
    assert_eq!(
        module_incoming("serial_com", "ports").unwrap(),
        ("APK.audio/System/Protocols/serial_com/incoming/ports".to_string(), true)
    );
    assert_eq!(
        module_port("serial_com", "dev:_dev_ttyS0").unwrap(),
        ("APK.audio/System/Protocols/serial_com/incoming/port/dev:_dev_ttyS0".to_string(), true)
    );
    assert!(module_port("serial_com", "a/b").is_err());
    assert_eq!(module_incoming("serial_com", "exchange").unwrap().1, false);
    assert_eq!(
        module_outgoing("serial_com", "open").unwrap(),
        ("APK.audio/System/Protocols/serial_com/outgoing/open".to_string(), false)
    );
    assert!(module_outgoing("serial_com", "exchange").is_err(), "exchange is an answer, not a request");
}
