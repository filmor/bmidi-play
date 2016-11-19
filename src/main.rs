//!
//!  play.rs
//!
//!  Based on synth/examples/test.rs by Mitchell Nordine
//!

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
use time_calc::{Bpm, Ppqn, Ticks};
use synth::{Synth, NoteFreqGenerator, mode};
use bmidi::{File, EventType, KeyEventType, Event};
use clap::App;
use std::sync::Arc;
use std::path::Path;

use futures::stream::{Stream, SendError, self};
use futures::Future;
use futures::Async;
use futures::task;
use futures::task::Executor;
use futures::task::Run;


struct MyExecutor;

impl Executor for MyExecutor {
    fn execute(&self, r: Run) {
        r.run();
    }
}


fn process_event<A: mode::Mode, B: NoteFreqGenerator, C, D, E, F>(
    evt: &Event, synth: &mut Synth<A, B, C, D, E, F>
    ) {

    // TODO: Pass interesting channel
    if evt.channel == 0 {
        if let EventType::Key{ typ, note, velocity } = evt.typ {
            println!("Key {:?} {:?} {}", typ, note, velocity);
            match typ {
                KeyEventType::Press => {
                    // FIXME: Conversion not working?!
                    let hz = note.to_step().to_hz().hz();
                    synth.note_on(hz, velocity as f32 / 256f32);
                },
                KeyEventType::Release => {
                    let hz = note.to_step().to_hz().hz();
                    synth.note_off(hz);
                }
                _ => {}
            }
        }
        else {
            println!("Ignored event {:?}", evt);
        }
    }
    else {
        println!("Ignored event {:?}", evt);
    }
}


fn run() -> Result<(), ()> {
    let matches = App::new("bmidi-play")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Simple midi player")
        .args_from_usage(
            "-t --track=[TRACK] 'The track to play'
             -c --channel=[CHANNEL] 'The channel in the track you want to hear'
             [FILENAME] 'A standard midi file'")
        .get_matches();

    let channel = value_t!(matches.value_of("CHANNEL"), u8).unwrap_or(0);
    let track = value_t!(matches.value_of("TRACK"), usize).unwrap_or(0);
    let filename = Arc::new(matches.value_of("FILENAME")
                            .expect("Missing filename"));


    let endpoint = cpal::get_default_endpoint().expect("Failed to get default endpoint");
    let format = endpoint.get_supported_formats_list().unwrap().next().expect("Failed to get endpoint format");

    let event_loop = cpal::EventLoop::new();
    let executor = Arc::new(MyExecutor);

    let (mut voice, stream) = cpal::Voice::new(&endpoint, &format, &event_loop).expect("Failed to create a voice");
    let samples_rate = format.samples_rate.0 as f32;


    // Construct our fancy Synth!
    let mut synth = {
        use synth::{Point, Oscillator, mode, oscillator, Envelope};

        let amp_env = Envelope::from(vec!(
            //         Time ,  Amp ,  Curve
            Point::new(0.0  ,  0.0 ,  0.0),
            Point::new(0.01 ,  1.0 ,  0.0),
            Point::new(0.45 ,  1.0 ,  0.0),
            Point::new(0.81 ,  0.8 ,  0.0),
            Point::new(1.0  ,  0.0 ,  0.0),
        ));

        // Now we can create our oscillator from our envelopes.
        // There are also Sine, Noise, NoiseWalk, SawExp and Square waveforms.
        let oscillator = Oscillator::new(oscillator::waveform::Sine, amp_env, 55., ());

        // Here we construct our Synth from our oscillator.
        Synth::new(mode::Poly, ())
            .oscillator(oscillator) // Add as many different oscillators as desired.
            .fade(50.0, 300.0) // Attack and Release in milliseconds.
            .num_voices(16) // By default Synth is monophonic but this gives it `n` voice polyphony.
            .volume(0.20)
            .spread(0.1)
    };

    let (tx, mut rx) = stream::channel();

    crossbeam::scope(|scope| {
        scope.spawn(|| -> Result<(), SendError<_, _>> {
            let res = File::parse(Path::new(filename.as_ref()));
            let track = res.track_iter(track);
            let ppqn = res.division as Ppqn;
            println!("PPQN: {:?}", ppqn);

            let mut tx = tx;

            for ev in track {
                println!("{:?}", ev);
                tx = tx.send(Ok(ev)).wait()?;
                // thread::sleep_ms(ev.delay);
            }

            tx.send(Err(())).wait()?;

            Ok(())
        });
    });

    let midi_tempo_to_bpm = |tempo| {
        // tempo is µs / beat (mus = 10^-6, min = 6 * 10^1 => min / mus = 6 * 10^7)
        // => bpm = (6 * 10^7) / tempo
        (6e7 / tempo) as Bpm
    };

    // TODO: Implement speed changes
    let bpm = midi_tempo_to_bpm(6e5);

    // How many frames do we still have to write with the current state?
    let mut cursor = 0 as i64;
    let mut next_cursor = 0 as i64;

    let mut current_event = Event{delay: 0, channel: channel, typ: EventType::SysEx};

    let callback = move |buffer| -> Result<_, ()> {
        match buffer {
            cpal::UnknownTypeBuffer::F32(mut buffer) => {
                let len = buffer.len() as i64;
                let mut inner_cursor = 0 as i64;
                
                while next_cursor < cursor + len - inner_cursor {
                    let frames = next_cursor - cursor;
                    let settings = Settings::new(
                        samples_rate as u32, frames as u16, 1
                        );

                    let new_output = &mut buffer[
                        inner_cursor as usize
                        ..(inner_cursor + frames) as usize ];

                    synth.audio_requested(new_output, settings);

                    inner_cursor += frames;
                    cursor = next_cursor;

                    match rx.poll() {
                        Ok(Async::Ready(Some(evt))) => {
                            println!("Got next event");

                            process_event(&current_event, &mut synth);

                            let skip = Ticks(evt.delay as i64)
                                .samples(bpm, 96 /* ppqn */,
                                         samples_rate as f64);

                            current_event = evt;

                            cursor = next_cursor;
                            next_cursor += skip;
                        }
                        _ => panic!("Ayyyeeee")
                    }

                    // TODO Get next event
                    /*let skip = Ticks(evt.delay as i64)
                        .samples(bpm, 120 /* ppqn*/, samples_rate as f64)
                        as u16;*/
                }

                if inner_cursor < len {
                    let settings = Settings::new(
                        samples_rate as u32, (len - inner_cursor) as u16, 1
                        );

                    let new_output = &mut buffer[
                        (inner_cursor as usize * 1) as usize
                        ..(len as usize * 1) as usize ];

                    synth.audio_requested(new_output, settings);
                }

                Ok(())
            }
            _ => Err(())
        }
        
    };

    voice.play();

    task::spawn(stream.for_each(callback)).execute(executor);

    event_loop.run();

    Ok(())
}

fn main() {
    run().expect("Error running");
}
