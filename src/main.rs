// set up USB: https://workshop.fomu.im/en/latest/requirements.html#setup-udev-rules (works for Ubuntu)

use std::io::{self, Write};
use wishbone_bridge::{UsbBridge, BridgeError};

fn main() -> Result<(), BridgeError> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Create a configuration object with a USB bridge that
    // connects to a device with the product ID of 0x5bf0.
    let bridge = UsbBridge::new().pid(0x5bf0).create()?;

    // Enable the oscillator. Note that this address may change,
    // so consult the `csr.csv` for your device.
    bridge.poke(0xf001_7000, 3 | (100 << 2) | (8 << 22))?;

    loop {
        // Wait until the `Ready` flag is `1`
        // while bridge.peek(0xf001_7008)? & 1 == 0 {}

        // Read the random word and write it to stdout
        handle
            .write_all(&bridge.peek(0xf001_7004)?.to_le_bytes())
            .unwrap();
    }
}
