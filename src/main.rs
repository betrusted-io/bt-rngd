// set up USB: https://workshop.fomu.im/en/latest/requirements.html#setup-udev-rules (works for Ubuntu)

use std::io::{self, Write};
use std::fs::File;
use wishbone_bridge::{UsbBridge, BridgeError};
use std::{thread, time};
use std::collections::HashMap;

fn do_vecs_match<T: PartialEq>(a: &Vec<T>, b: &Vec<T>) -> bool {
    let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
    matching == a.len() && matching == b.len()
}

/// purpose of this is to find USB link issues. Suspecting that over very long
/// runs, we are having some bad blocks that have repeated elements. Capture for later
/// diagnosis.
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

fn get_base(value: &str) -> (&str, u32) {
    if value.starts_with("0x") {
        (value.trim_start_matches("0x"), 16)
    } else if value.starts_with("0X") {
        (value.trim_start_matches("0X"), 16)
    } else if value.starts_with("0b") {
        (value.trim_start_matches("0b"), 2)
    } else if value.starts_with("0B") {
        (value.trim_start_matches("0B"), 2)
    } else if value.starts_with('0') && value != "0" {
        (value.trim_start_matches('0'), 8)
    } else {
        (value, 10)
    }
}
fn parse_u32(value: &str) -> u32 {
    let (value, base) = get_base(value);
    u32::from_str_radix(value, base).unwrap()
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

    let maybe_csr_data = bridge.burst_read(0x2027_8000, 0x8000 as u32);

    let mut mapping = HashMap::new();

    use std::convert::TryInto;
    let csr_data: [u8; 0x8000] = if let Ok(data) = maybe_csr_data {
        data[0..0x8000].try_into().unwrap()
    } else {
        [0 as u8; 0x8000]
    };
    use sha2::{Sha512, Digest};
    let mut hasher = Sha512::new();
    let hash_region: [u8; 0x7fc0] = csr_data[..0x7fc0].try_into().unwrap();
    hasher.update(hash_region);
    let result = hasher.finalize();
    let mut hash_index = 0x7fc0;
    let mut matched = true;
    for b in result {
        if b != csr_data[hash_index] {
            log::trace!("CSR mismatch at 0x{:x}. Read 0x{:x}, computed 0x{:x}", hash_index, csr_data[hash_index], b);
            matched = false;
        }
        hash_index += 1;
    }
    if matched {
        log::trace!("Good hash check on CSR map stored on device.");
        let csr_len = u32::from_le_bytes(csr_data[0..4].try_into().unwrap()) as usize;
        let csr = &csr_data[4..4+csr_len];
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(csr);
        for result in rdr.records() {
            if let Ok(r) = result {
                match &r[0] {
                    "csr_register" => {
                        let reg_name = &r[1];
                        let base_addr = parse_u32(&r[2]);
                        let num_regs = parse_u32(&r[3]);

                        // If there's only one register, add it to the map.
                        // However, CSRs can span multiple registers, and do so in reverse.
                        // If this is the case, create indexed offsets for those registers.
                        match num_regs {
                            1 => {
                                mapping.insert(reg_name.to_string().to_lowercase(), Some(base_addr));
                            }
                            n => {
                                mapping.insert(reg_name.to_string().to_lowercase(), Some(base_addr));
                                for logical_reg in 0..n {
                                    mapping.insert(
                                        format!(
                                            "{}{}",
                                            reg_name.to_string().to_lowercase(),
                                            n - logical_reg - 1
                                        ),
                                        Some(base_addr + logical_reg * 4),
                                    );
                                }
                            }
                        }
                    }
                    "memory_region" => {
                        let region = &r[1];
                        let base_addr = parse_u32(&r[2]);
                        mapping.insert(region.to_string().to_lowercase(), Some(base_addr));
                    }
                    "csr_base" => {
                        let region = &r[1];
                        let base_addr = parse_u32(&r[2]);
                        mapping.insert(region.to_string().to_lowercase(), Some(base_addr));
                    }
                    _ => (),
                };
            }
        }
    } else {
        eprintln!("Couldn't read CSR data out of device, quitting!");
        std::process::exit(1);
    }

    //let messible2_in = 0xf001_0000; // replace with messible2_in
    let messible2_in = mapping.get("messible2_in").unwrap().unwrap() as u32;
    //let messible_out = 0xf000_f004;
    let messible_out = mapping.get("messible_out").unwrap().unwrap() as u32;

    let mut blocks = 0;
    let mut phase = 0;
    let mut old_a: Vec<u8> = Vec::new();
    let mut old_b: Vec<u8> = Vec::new();
    loop {
        // wait for a new phase -- don't check the "have", just read the fifo, b/c USB packets are expensive
    	let timeout = time::Duration::from_millis(60_000);
        let now = time::Instant::now();
        loop {
            let mval = bridge.peek(messible_out)?;
            if mval > phase {
                phase = mval;
                break;
            }
            thread::sleep(time::Duration::from_millis(5));
            //eprintln!("waiting for {}, got {} ", phase, bridge.peek(messible_out)?);
            if now.elapsed() >= timeout {
                eprintln!("Timeout synchronizing phase, advancing phase counter anyways.");
                write!(logfile, "Timeout synchronizing phase, advancing phase counter anyways.\n").unwrap();
                phase = mval;
                bridge.poke(messible2_in, phase)?;
                break;
            }
        }
        // dispatch on phase
        if phase % 2 == 1 {
            bridge.poke(messible2_in, phase)?; // sending anything increments the buffer phase

            // read A concurrently with B fill
            let page = match bridge.burst_read(ram_a, burst_len) {
                Err(e) => {
                    eprintln!("USB bridge error {}, ignoring packet (phase 1)", e);
		    write!(logfile, "USB bridge error, ignoring packet\n").unwrap();
                    vec![] // just skip this data and return the next packet
                },
                Ok(data) => data
            };

            // flush data to stdout
            // eprintln!("read {} bytes", page.len());
            if block_is_good(&page) {
                if do_vecs_match(&page, &old_a) || do_vecs_match(&page, &old_b) {
                    write!(logfile, "exact match found (protocol error), skipping").unwrap();
                } else {
                    handle.write_all(&page)?;
                    blocks += 1;
                    write!(logfile, "block {} ok\n", blocks).unwrap();
                }
            } else {
                diag_print(&mut logfile, &page, blocks);
            }
            old_a = page.clone();
        } else {
            bridge.poke(messible2_in, phase)?; // sending anything increments the buffer phase

            // read B while A fills
            let page = match bridge.burst_read(ram_b, burst_len) {
                Err(e) => {
		    write!(logfile, "USB bridge error, ignoring packet\n").unwrap();
                    eprintln!("USB bridge error {}, ignoring packet (phase 2)", e);
                    vec![]
                },
                Ok(data) => data
            };

            // flush data to stdout
            // eprintln!("read {} bytes", page.len());
            if block_is_good(&page) {
                if do_vecs_match(&page, &old_a) || do_vecs_match(&page, &old_b) {
                    write!(logfile, "exact match found (protocol error), skipping").unwrap();
                } else {
                    handle.write_all(&page)?;
                    blocks += 1;
                    write!(logfile, "block {} ok\n", blocks).unwrap();
                }
            } else {
                diag_print(&mut logfile, &page, blocks);
            }
            old_b = page.clone();
        }
        logfile.sync_all().unwrap();
    }
}
