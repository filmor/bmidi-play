// extern crate dsp;
extern crate synth;
extern crate time_calc;
extern crate dsp;
extern crate bmidi;
extern crate cpal;
extern crate futures;
extern crate crossbeam;

#[macro_use]
extern crate clap;

use dsp::{Node, Settings};
use time_calc::{Bpm, Ticks};
use bmidi::{EventType, Event};
use clap::App;
use std::sync::Arc;
use std::path::Path;

use futures::stream::Stream;
use futures::Async;
use futures::task::{self, Run, Executor};
use futures::sync::mpsc;
use std::cmp;

mod synth_util;
mod sink;
mod source;

use sink::Sink as MySink;

struct MyExecutor;

impl Executor for MyExecutor {
    fn execute(&self, r: Run) {
        r.run();
    }
}


fn run() -> Result<(), ()> {
    let matches = App::new("bmidi-play")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Simple midi player")
        .args_from_usage(
            "-t, --track=[TRACK] 'The track to play'
             -c, --channel=[CHANNEL] 'The channel in the track you want to hear'
             [FILENAME] 'A standard midi file'")
        .get_matches();

    let channel = value_t!(matches.value_of("CHANNEL"), u8).unwrap_or(0);
    let track = value_t!(matches.value_of("TRACK"), usize).unwrap_or(0);
    let filename = matches.value_of("FILENAME").expect("Missing filename");

    println!("Playing track {} of {}", track, filename);

    let mut synth = synth_util::new();

    let endpoint = cpal::get_default_endpoint().expect("Failed to get default endpoint");
    let format = endpoint.get_supported_formats_list()
        .unwrap()
        .next()
        .expect("Failed to get endpoint format");

    let event_loop = cpal::EventLoop::new();
    let executor = Arc::new(MyExecutor);

    let (mut voice, stream) = cpal::Voice::new(&endpoint, &format, &event_loop)
        .expect("Failed to create a voice");
    let samples_rate = format.samples_rate.0 as f32;
    println!("Sample rate: {}", samples_rate);

    let (tx, mut rx) = mpsc::channel(4);

    println!("Channel filling");

    let midi_tempo_to_bpm = |tempo: f32| {
        // tempo is µs / beat (mus = 10^-6, min = 6 * 10^1 => min / mus = 6 * 10^7)
        // => bpm = (6 * 10^7) / tempo
        (6e7 / tempo) as Bpm
    };

    // TODO: Implement speed changes
    let bpm = midi_tempo_to_bpm(6e4);

    // How many frames do we still have to write with the current state?
    let mut cursor = 0 as i64;
    let mut next_cursor = 0 as i64;

    let mut current_event = Event {
        delay: 0,
        channel: channel,
        typ: EventType::SysEx,
    };

    crossbeam::scope(|scope| {
        source::fill_channel(scope, tx, Path::new(filename), track);

    let callback = move |buffer: cpal::UnknownTypeBuffer| -> Result<_, ()> {
        let len = buffer.len() as i64;

        match buffer {
            cpal::UnknownTypeBuffer::I16(mut buffer) => {
                println!("Got i16 buffer of length {}", len);
                let mut inner_cursor = 0 as i64;
                let start_cursor = cursor;

                let mut loops = 0;

                while next_cursor < start_cursor + len {
                    loops += 1;
                    // println!("\n\nstart: {}\nnext: {}\ncurrent: {}\ninner: {}\nlen: {}", start_cursor, next_cursor, cursor, inner_cursor, len);
                    let frames = cmp::min(next_cursor - cursor, len - inner_cursor);

                    let settings = Settings::new(samples_rate as u32, frames as u16, 1);

                    let new_output =
                        &mut buffer[inner_cursor as usize..(inner_cursor + frames) as usize];

                    synth.audio_requested(new_output, settings);

                    inner_cursor += frames;
                    cursor = next_cursor;

                    match rx.poll() {
                        Ok(Async::Ready(Some(evt))) => {
                            // println!("Got next event: {:?}", evt);

                            synth.process_event(&current_event);

                            let skip = Ticks(evt.delay as i64)
                                .samples(bpm, 96 /* ppqn */, samples_rate as f64);

                            current_event = evt;

                            cursor = next_cursor;
                            next_cursor += skip;
                        }
                        Ok(Async::Ready(None)) => {
                            panic!("Stop it!");
                        }
                        var => {
                            println!("Unprocessed result: {:?}", var);
                            return Ok(());


                        } //panic!("Ayyyeeee")
                    }
                }

                if inner_cursor < len {
                    let settings =
                        Settings::new(samples_rate as u32, (len - inner_cursor) as u16, 1);

                    let new_output = &mut buffer[inner_cursor as usize..len as usize];

                    synth.audio_requested(new_output, settings);
                }

                cursor += len - inner_cursor;

                // println!("Processed {} events, advanced cursor by {}", loops, len);

                Ok(())
            }
            _ => Err(()),
        }

    };

    println!("Starting to play");
    voice.play();
    println!("Spawning callback");
    task::spawn(stream.for_each(callback)).execute(executor);

    println!("Starting event loop");

    event_loop.run();

    Ok(())
    })
}

fn main() {
    run().expect("Error running");
}
