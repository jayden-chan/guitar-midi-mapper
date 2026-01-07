use gilrs::{Event, EventType, Gilrs};
use midir::{MidiOutput, MidiOutputPort};
use std::io::{stdin, stdout, Write};
use std::sync::mpsc::channel;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

fn main() {
    let mut gilrs = Gilrs::new().unwrap();

    for (id, gamepad) in gilrs.gamepads() {
        println!("{} {} {:?}", id, gamepad.name(), gamepad.power_info());
    }

    print!("Please select gamepad: ");
    stdout().flush().unwrap();
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();

    let selected_id: usize = input.trim().parse().unwrap();

    let midi_out = MidiOutput::new("Guitar Midi Mapper").unwrap();

    let out_ports = midi_out.ports();
    let out_port: &MidiOutputPort = match out_ports.len() {
        0 => panic!("no output port found"),
        1 => {
            println!(
                "Choosing the only available output port: {}",
                midi_out.port_name(&out_ports[0]).unwrap()
            );
            &out_ports[0]
        }
        _ => {
            println!("\nAvailable output ports:");
            for (i, p) in out_ports.iter().enumerate() {
                println!("{}: {}", i, midi_out.port_name(p).unwrap());
            }

            print!("Please select output port: ");
            stdout().flush().unwrap();
            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();

            out_ports
                .get(input.trim().parse::<usize>().unwrap())
                .ok_or("invalid output port selected")
                .unwrap()
        }
    };

    println!("\nOpening connection");
    let mut conn_out = midi_out.connect(out_port, "guitar-midi-mapper").unwrap();
    println!("Connection open. Listening for inputs");

    const NOTE_ON_MSG: u8 = 0b1001_0000;
    const NOTE_OFF_MSG: u8 = 0b1000_0000;
    const CTRL_CHANGE_MSG: u8 = 0b1011_0000;
    const VELOCITY: u8 = 0x64;

    let (tx, rx) = channel();
    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    loop {
        let ctrlc_pressed = rx.try_recv();
        if ctrlc_pressed.is_ok() {
            break;
        }

        while let Some(Event {
            id, event, time, ..
        }) = gilrs.next_event()
        {
            if usize::from(id) != selected_id {
                continue;
            }

            match event {
                EventType::ButtonPressed(_button, code) => {
                    let mapped_code = (code.into_u32() % 255) as u8;
                    let _ = conn_out.send(&[NOTE_ON_MSG, mapped_code, VELOCITY]);
                }
                EventType::ButtonReleased(_button, code) => {
                    let mapped_code = (code.into_u32() % 255) as u8;
                    let _ = conn_out.send(&[NOTE_OFF_MSG, mapped_code, VELOCITY]);
                }
                EventType::AxisChanged(_axis, value, code) => {
                    let mapped_code = (code.into_u32() % 255) as u8;
                    if mapped_code == 6 {
                        let mapped_value = (time
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                            % 127) as u8;

                        let _ = conn_out.send(&[CTRL_CHANGE_MSG, mapped_code, mapped_value]);
                    } else if mapped_code == 7 && value > 0.85 {
                        let _ = conn_out.send(&[NOTE_ON_MSG, mapped_code, VELOCITY]);
                        sleep(Duration::from_millis(1));
                        let _ = conn_out.send(&[NOTE_OFF_MSG, mapped_code, VELOCITY]);
                    }
                }
                _ => {}
            }
        }

        sleep(Duration::from_millis(1));
    }

    conn_out.close();
    println!("MIDI Connection closed");
}
