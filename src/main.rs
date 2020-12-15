// set up USB: https://workshop.fomu.im/en/latest/requirements.html#setup-udev-rules (works for Ubuntu)

use std::io::{self, Write};
use std::fs::File;
use wishbone_bridge::{UsbBridge, BridgeError};
use std::{thread, time};

fn block_is_good(block: &Vec<u8>) -> bool {
    let mut maxrun = 0;
    let mut histogram: [usize; 256] = [0; 256];
    let total = block.len();

    let mut last: u8 = 0;
    let mut lastrun = 0;
    for val in block.iter() {
        // runs check on a u8 -- this is a pathology of USB getting stuck
        if *val == last {
            lastrun += 1;
        } else {
            if lastrun > maxrun {
                maxrun = lastrun;
            }
            lastrun = 0;
            last = *val;
        }

        histogram[*val as usize] += 1;
    }

    let mut max = 0;
    for i in 0..256 {
        if histogram[i] > max {
            max = histogram[i];
        }
    }
    if max > ((total / 256) * 4) {
        return false;
    }
    if maxrun > 4 {
        return false;
    }

    true
}

fn diag_print(logfile: &mut File, block: &Vec<u8>, blocks: i32) {
    write!(logfile, "Suspicious block found block {}, suppressing.\n", blocks).unwrap();
    write!(logfile, "Block for guru meditation:").unwrap();
    let mut index = 0;
    for data in block.iter() {
        if (index % 64) == 0 {
            write!(logfile, "\n0x{:05x}: ", index).unwrap();
        }
        index += 1;
        write!(logfile, "{:02x} ", *data).unwrap();
    }
    write!(logfile, "\n\n").unwrap();
}

fn main() -> Result<(), BridgeError> {
    let mut logfile = File::create("log.txt")?;

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Create a configuration object with a USB bridge that
    // connects to a device with the product ID of 0x5bf0.
    let bridge = UsbBridge::new().pid(0x5bf0).create()?;

    let ram_a = 0x4020_0000;
    let ram_b = 0x4030_0000;
    let burst_len = 512 * 1024;

    let messible2_in = 0xf001_1000; // replace with messible2_in
    let messible_out = 0xf001_0004;

    /*
    loop {
        let page = bridge.burst_read(ram_a, burst_len).unwrap();
        // flush data to stdout
        handle.write_all(&page)?;
    }*/

    let mut blocks = 0;
    let mut phase = 0x1;
    bridge.poke(messible2_in, 2)?;  // start with a fill request for the next phase, so we can meet the lockstep criteria
    loop {
        // dispatch on phase
        if phase == 1 {
	    // in this case, we must have requested B buffer to fill, because we got a B (that is, 2) ACK
            bridge.poke(messible2_in, 1)?; // sending 1 request concurrent fill of A while we read B

            // read B concurrently with A fill
            let page = match bridge.burst_read(ram_b, burst_len) {
                Err(e) => {
                    eprintln!("USB bridge error {}, ignoring packet (phase 1)", e);
                    vec![] // just skip this data and return the next packet
                },
                Ok(data) => data
            };

            phase = 2;

            // flush data to stdout
            // eprintln!("read {} bytes", page.len());
            if block_is_good(&page) {
                handle.write_all(&page)?;
                blocks += 1;
                write!(logfile, "block {} ok\n", blocks).unwrap();
            } else {
                diag_print(&mut logfile, &page, blocks);
            }
        } else {
            bridge.poke(messible2_in, 2)?; // sending 2 concurrently fills B

            // read A while B fills
            let page = match bridge.burst_read(ram_a, burst_len) {
                Err(e) => {
                    eprintln!("USB bridge error {}, ignoring packet (phase 2)", e);
                    vec![]
                },
                Ok(data) => data
            };

            phase = 1;

            // flush data to stdout
            // eprintln!("read {} bytes", page.len());
            if block_is_good(&page) {
                handle.write_all(&page)?;
                blocks += 1;
                write!(logfile, "block {} ok\n", blocks).unwrap();
            } else {
                diag_print(&mut logfile, &page, blocks);
            }
        }
        logfile.sync_all().unwrap();

        // wait for a new phase -- don't check the "have", just read the fifo, b/c USB packets are expensive
    	let timeout = time::Duration::from_millis(20_000);
    	let now = time::Instant::now();
        while phase != bridge.peek(messible_out)? {
            thread::sleep(time::Duration::from_millis(5));
            //eprintln!("waiting for {}, got {} ", phase, bridge.peek(messible_out)?);
            if now.elapsed() >= timeout {
                eprintln!("Timeout synchronizing phase, advancing phase counter anyways.");
                break;
            }
        }
    }
}
