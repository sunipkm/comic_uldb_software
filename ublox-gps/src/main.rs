use std::{
    io::{ErrorKind, Write},
    time::{Duration, Instant},
};

use ublox_gps_tec::parse_messages;

fn main() {
    let mut serial_port = serialport::new("/dev/ttyUSB0", 115200)
        .open()
        .expect("Failed to open serial port");
    serial_port
        .set_timeout(Duration::from_secs_f32(0.1))
        .expect("Failed to set timeout");
    let mut ofile = std::fs::File::create("gps_data.bin").expect("Failed to create file");
    let mut now = Instant::now();
    loop {
        let last = now;
        let mut buf = Vec::with_capacity(1024);
        if let Err(err) = serial_port.read_to_end(&mut buf) {
            if err.kind() != ErrorKind::TimedOut {
                eprintln!("Failed to read from serial port: {}", err);
                break;
            }
        }
        if buf.is_empty() {
            continue;
        }
        if ofile.write_all(&buf).is_err() {
            println!("Failed to write to file");
        }
        if ofile.write_all(b"\r\r\n\n\r\r\n\n").is_err() {
            println!("Failed to write separator to file");
        }
        println!("----------------------------------------");
        print!("Read: {} bytes\t", buf.len());
        now = Instant::now();
        println!("Time: {} s", (now - last).as_secs_f32());
        let ubxinfo = parse_messages(buf);
        match ubxinfo {
            Ok(ubxinfo) => {
                let (mfcount, mfsats) =
                    ubxinfo
                        .carrier_phase()
                        .iter()
                        .fold((0, Vec::new()), |mut count, (sat, ch)| {
                            if ch.meas.len() > 1 {
                                count.0 += 1;
                                count.1.push(sat);
                            }
                            count
                        });
                println!(
                    "Timestamp: {:?}, Location: {:?}, Multi-frequency: {}/{}, Satellites: {:?}",
                    ubxinfo.timestamp(),
                    ubxinfo.location(),
                    mfcount,
                    ubxinfo.carrier_phase().len(),
                    mfsats
                );
            }
            Err(e) => {
                eprintln!("Error: {:?}", e);
            }
        }
        println!("++++++++++++++++++++++++++++++++++++++++");
        // println!("{:?}", buf);
    }
}
