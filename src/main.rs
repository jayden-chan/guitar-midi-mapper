use gilrs::{Event, EventType, Gilrs};
use midir::{MidiOutput, MidiOutputPort};
use regex::Regex;
use std::env;
use std::sync::mpsc::channel;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        let gilrs = Gilrs::new().unwrap();
        let mut found_gamepad = false;

        println!("Available Gamepads:");
        for (id, gamepad) in gilrs.gamepads() {
            println!("  - {} (ID: {})", gamepad.name(), id);
            found_gamepad = true;
        }

        if !found_gamepad {
            println!("  (No gamepads detected)");
        }

        let midi_out = MidiOutput::new("Guitar Midi Mapper").unwrap();
        let out_ports = midi_out.ports();

        println!("\nAvailable MIDI Output Ports:");
        if out_ports.is_empty() {
            println!("  (No MIDI output ports detected)");
        } else {
            for p in out_ports {
                println!("  - {}", midi_out.port_name(&p).unwrap());
            }
        }

        return;
    }

    if args.len() != 3 {
        eprintln!("Usage: {} <gamepad_regex> <midi_regex>", args[0]);
        std::process::exit(1);
    }

    let gamepad_re = Regex::new(&args[1]).expect("Invalid gamepad regex");
    let midi_re = Regex::new(&args[2]).expect("Invalid MIDI regex");

    let mut gilrs = Gilrs::new().unwrap();

    let matching_gamepads: Vec<_> = gilrs
        .gamepads()
        .filter(|(_, gamepad)| gamepad_re.is_match(gamepad.name()))
        .collect();

    let selected_id = match matching_gamepads.len() {
        0 => panic!("Gamepad '{}' not found", args[1]),
        1 => {
            let (id, gamepad) = matching_gamepads[0];
            println!("Selected gamepad: {} (ID: {})", gamepad.name(), id);
            usize::from(id)
        }
        _ => {
            eprintln!("Multiple matching gamepads found:");
            for (id, gamepad) in matching_gamepads {
                eprintln!("  - {} (ID: {})", gamepad.name(), id);
            }
            panic!("Multiple matching gamepads found");
        }
    };

    let midi_out = MidiOutput::new("Guitar Midi Mapper").unwrap();
    let out_ports = midi_out.ports();

    let matching_midi_ports: Vec<_> = out_ports
        .iter()
        .filter(|p| {
            let name = midi_out.port_name(p).unwrap();
            midi_re.is_match(&name)
        })
        .collect();

    let out_port: &MidiOutputPort = match matching_midi_ports.len() {
        0 => panic!("No matching MIDI output port found for regex: {}", args[2]),
        1 => {
            let port = matching_midi_ports[0];
            println!(
                "Selected MIDI output port: {}",
                midi_out.port_name(port).unwrap()
            );
            port
        }
        _ => {
            eprintln!("Multiple matching MIDI output ports found:");
            for p in matching_midi_ports {
                eprintln!("  - {}", midi_out.port_name(p).unwrap());
            }
            panic!("Multiple matching MIDI output ports found");
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
