// set up USB: https://workshop.fomu.im/en/latest/requirements.html#setup-udev-rules (works for Ubuntu)

use std::io::{self, Write};
use wishbone_bridge::{UsbBridge, BridgeError};

fn main() -> Result<(), BridgeError> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Create a configuration object with a USB bridge that
    // connects to a device with the product ID of 0x5bf0.
    let bridge = UsbBridge::new().pid(0x5bf0).create()?;

    let ram_a = 0x4008_0000;
    let burst_len = 512 * 1024;

    loop {
        let page = bridge.burst_read(ram_a, burst_len).unwrap();
        // flush data to stdout
        handle.write_all(&page)?;
    }
    
    /* read sync doesn't work due to caching
    let ram_b = 0x4010_0000;
    let read_sync = 0x4007_0000;
    let messible_out = 0xf000f004;

    let mut last_phase = 0x40; // unit state
    loop {
        // wait for a new phase
        loop {
	   let cur_phase = bridge.peek(messible_out)?;
	   if cur_phase != last_phase {
	      last_phase = cur_phase;
	      break;
	   }
        }
	// dispatch on phase
        if last_phase == 0x41 {
	   eprintln!("A");
	   let page = bridge.burst_read(ram_a, burst_len).unwrap();
	   
   	   // update read ack
	   bridge.poke(read_sync, 1)?;
	   
	   // flush data to stdout
	   handle.write_all(&page)?;
	} else {
	   eprintln!("B");
	   let page = bridge.burst_read(ram_b, burst_len).unwrap();
	   
   	   // update read ack
	   bridge.poke(read_sync, 1)?;
	   
	   // flush data to stdout
	   handle.write_all(&page)?;
	}
    } */
}
