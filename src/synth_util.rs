extern crate synth;
extern crate bmidi;

use sink::Sink;
use synth::{Synth, NoteFreqGenerator, mode, oscillator, Envelope};
use bmidi::{EventType, KeyEventType};


pub fn new() -> Synth<mode::Poly, (), oscillator::waveform::Sine, Envelope, f64, ()> {
    // Construct our fancy Synth!
    use synth::{Synth, Point, Oscillator, mode, oscillator, Envelope};

    let amp_env = Envelope::from(vec![//         Time ,  Amp ,  Curve
                                      Point::new(0.0, 0.0, 0.0),
                                      Point::new(0.01, 1.0, 0.0),
                                      Point::new(0.45, 1.0, 0.0),
                                      Point::new(0.81, 0.8, 0.0),
                                      Point::new(1.0, 0.0, 0.0)]);

    // Now we can create our oscillator from our envelopes.
    // There are also Sine, Noise, NoiseWalk, SawExp and Square waveforms.
    let oscillator = Oscillator::new(oscillator::waveform::Sine, amp_env, 55., ());

    // Here we construct our Synth from our oscillator.
    Synth::new::<>(mode::Poly, ())
        .oscillator(oscillator) // Add as many different oscillators as desired.
        .fade(50.0, 300.0) // Attack and Release in milliseconds.
        .num_voices(16) // By default Synth is monophonic but this gives it `n` voice polyphony.
        .volume(0.20)
        .spread(0.1)
}


impl<A: mode::Mode, B: NoteFreqGenerator, C, D, E, F> Sink for Synth<A, B, C, D, E, F> {
    fn process_event(&mut self, evt: &bmidi::Event) {
        // TODO: Pass interesting channel
        if evt.channel == 0 {
            if let EventType::Key { typ, note, velocity } = evt.typ {
                // println!("Key {:?} {:?} {}", typ, note, velocity);
                let hz = note.to_step().to_hz().hz();
                match typ {
                    KeyEventType::Press => {
                        // FIXME: Conversion not working?!
                        self.note_on(hz, velocity as f32 / 256f32);
                    }
                    KeyEventType::Release => {
                        self.note_off(hz);
                    }
                    _ => {}
                }
            } else {
                // println!("Ignored event {:?}", evt);
            }
        } else {
            // println!("Ignored event {:?}", evt);
        }
    }
}
