// set up USB: https://workshop.fomu.im/en/latest/requirements.html#setup-udev-rules (works for Ubuntu)

use std::io::{self, Write};
use wishbone_bridge::{UsbBridge, BridgeError};
use std::{thread, time};

fn main() -> Result<(), BridgeError> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Create a configuration object with a USB bridge that
    // connects to a device with the product ID of 0x5bf0.
    let bridge = UsbBridge::new().pid(0x5bf0).create()?;

    let ram_a = 0x4008_0000;
    let burst_len = 512 * 1024;

    /*
    loop {
        let page = bridge.burst_read(ram_a, burst_len).unwrap();
        // flush data to stdout
        handle.write_all(&page)?;
    }*/

    let ram_b = 0x4010_0000;
    let messible2_in = 0xf001_0000; // replace with messible2_in
    let messible_out = 0xf000_f004;

    let mut phase = 0x1;
    bridge.poke(messible2_in, 1)?;
    bridge.poke(messible2_in, 2)?;
    loop {
        // wait for a new phase
        while phase != bridge.peek(messible_out)? {
            thread::sleep(time::Duration::from_millis(10));
            //eprintln!("waiting for {}, got {} ", phase, bridge.peek(messible_out)?);
        }
        thread::sleep(time::Duration::from_millis(5));

        // dispatch on phase
        if phase == 1 {
            bridge.poke(messible2_in, 2)?; // sending 2 fills B
            // so read A
            let page = match bridge.burst_read(ram_a, burst_len) {
                Err(e) => {
                    eprintln!("USB bridge error {}, ignoring packet (phase 1)", e);
                    vec![] // just skip this data and return the next packet
                },
                Ok(data) => data
            };

            phase = 2;

            // flush data to stdout
            //eprintln!("read {} bytes", page.len());
            handle.write_all(&page)?;
        } else {
            bridge.poke(messible2_in, 1)?; // sending 1 fills A
            // so read B
            let page = match bridge.burst_read(ram_b, burst_len) {
                Err(e) => {
                    eprintln!("USB bridge error {}, ignoring packet (phase 2)", e);
                    vec![]
                },
                Ok(data) => data
            };

            phase = 1;

            // flush data to stdout
            //eprintln!("read {} bytes", page.len());
            handle.write_all(&page)?;
        }
        thread::sleep(time::Duration::from_millis(5));
    }
}
