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
extern crate portaudio;

#[macro_use]
extern crate clap;

use dsp::{Graph, Node, Frame, FromSample, Sample, Walker};
use dsp::sample::ToFrameSliceMut;
use time_calc::{Bpm, Ppqn, Ticks};
use portaudio as pa;
use synth::Synth;
use bmidi::{File, EventType, KeyEventType};
use std::cmp;
use clap::App;

const CHANNELS: usize = 2;
const FRAMES: u32 = 256;
const SAMPLE_HZ: f64 = 44_100.0;

// Currently supports i8, i32, f32.
pub type AudioSample = f32;
pub type Input = AudioSample;
pub type Output = AudioSample;

fn run() -> Result<(), pa::Error> {
    let matches = App::new("bmidi-play")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Simple midi player")
        .args_from_usage(
            "-t --track=[TRACK] 'The track to play'
             -c --channel=[CHANNEL] 'The channel in the track you want to hear'
             [FILENAME] 'A standard midi file'")
        .get_matches();

    let channel = value_t!(matches.value_of("CHANNEL"), u8).unwrap_or(0);
    let track = value_t!(matches.value_of("TRACK"), usize).unwrap_or(1);
    let filename = matches.value_of("FILENAME").unwrap();

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

    // We'll use this to keep track of time and break from the loop after 6 seconds.
    let res = File::parse(filename.as_ref());
    let mut track = res.track_iter(track).peekable();

    let ppqn = res.division as Ppqn;
    let mut bpm = 120.0 as Bpm;

    let midi_tempo_to_bpm = |tempo| {
        // tempo is µs / beat (mus = 10^-6, min = 6 * 10^1 => min / mus = 6 * 10^7)
        // => bpm = (6 * 10^7) / tempo
        (6e7 / tempo) as Bpm
    };

    bpm = midi_tempo_to_bpm(6e5);

    // How many frames do we still have to write with the current state?
    let mut cursor = 0 as i64;

    let callback = move |pa::OutputStreamCallbackArgs { buffer, frames, time, .. }| {
        let buffer: &mut [[Output; CHANNELS as usize]]
            = buffer.to_frame_slice_mut().unwrap();

        dsp::slice::equilibrium(buffer);

        let mut inner_cursor: i64 = 0;

        while inner_cursor < buffer.len() as i64 {
            if cursor <= 0 {
                let evt = track.next().unwrap();

                if evt.channel == channel {
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

                if let Some(next_evt) = track.peek() {
                    // TODO Modify bpm using SetTempo events, for that we need
                    //      to iterate over all tracks at once (FF 51 03 + 24bit,
                    //      microseconds per quarter node)
                    let skip = Ticks(next_evt.delay as i64)
                        .samples(bpm, ppqn, SAMPLE_HZ as f64)
                        as u16;

                    cursor += skip as i64;
                }
                else {
                    return pa::Complete
                }
            }

            let new_inner_cursor = cmp::min(
                inner_cursor as i64 + cursor,
                frames as i64
                );

            let (begin, end) = (inner_cursor as i64, new_inner_cursor as i64);

            let frames = end - begin;

            let new_output = &mut buffer[
                (begin as usize * CHANNELS) as usize
                ..(end as usize * CHANNELS) as usize ];
           
            // FIXME: Write the actual audio data
            // synth.audio_requested();
            // synth.audio_requested(new_output, SAMPLE_HZ);

            cursor -= (new_inner_cursor - inner_cursor) as i64;
            inner_cursor = new_inner_cursor;
        }

        pa::Continue
    };

    // Construct PortAudio and the stream.
    let pa = try!(pa::PortAudio::new());
    let settings = try!(
        pa.default_output_stream_settings::<f32>(
            CHANNELS as i32,
            SAMPLE_HZ,
            FRAMES)
        );
    let mut stream = try!(pa.open_non_blocking_stream(settings, callback));
    try!(stream.start());

    // Loop while the stream is active.
    while let Ok(true) = stream.is_active() {}

    Ok(())
}

fn main() {
    run();
}
