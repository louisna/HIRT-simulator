#[macro_use]
extern crate log;

use std::hash::Hash;
use std::hash::Hasher;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Forward,

    FecEncoder(String),

    FecDecoder(String),

    FecDoubleMetadata,

    FecWrongMetadata,

    FeedbackIdTooBig,

    UnusedRepair,

    TooOldEquation,
}

#[derive(Default, Clone, Debug)]
/// Simple structure representing a packet. It contains a unique ID used for the simulation and FEC scheme-specific metadata.
pub struct Packet {
    id: u64,
    fec: Option<FecMetadata>,
    recovered: Option<u64>, // Distance from its ID where it has been recovered.

    data: Vec<u8>,
}

impl Packet {
    /// New packet from ID.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            data: id.to_be_bytes().to_vec(),
            ..Default::default()
        }
    }

    pub fn new_recovered(id: u64, from: u64) -> Self {
        let mut pkt = Self::new(id);
        pkt.recovered = Some(from.saturating_sub(id));
        pkt
    }
}

impl PartialEq for Packet {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.data == other.data
    }
}

impl Eq for Packet {}

impl Hash for Packet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.data.hash(state);
    }
}

/// Contains all nodes and parameters to start the simulation.
pub struct Simulator {
    /// Source node.
    source: Source,

    /// Encoder.
    encoder: Encoder,

    /// Dropper.
    dropper: Dropper,

    /// Decoder.
    decoder: Decoder,

    /// Sink node.
    sink: Sink,
}

impl Simulator {
    pub fn new() -> Self {
        Self {
            source: Source::new(),
            encoder: Encoder::new_simple(),
            dropper: Dropper::new_simple(),
            decoder: Decoder::new_simple(),
            sink: Sink::new(),
        }
    }

    pub fn run(&mut self, nb_packets: u64) -> Result<()> {
        for _iter in 0..nb_packets {
            // Generate the packet from the source.
            let packets = vec![self.source.gen()];

            self.encoder.recv(packets)?;
            let packets = self.encoder.forw()?;

            self.dropper.recv(packets)?;
            let packets = self.dropper.forw()?;

            self.decoder.recv(packets)?;
            let (packets, feedback) = self.decoder.forw()?;

            // Potentially give feedback to encoder.
            if !feedback.is_empty() {
                self.encoder.recv_feedback(feedback);
            }

            // Give the ouptut packets to the sink.
            self.sink.recv_multiple(packets);
        }

        Ok(())
    }

    pub fn get_sink(&self) -> &Sink {
        &self.sink
    }

    pub fn set_encoder(&mut self, encoder: Encoder) {
        self.encoder = encoder;
    }

    pub fn set_dropper(&mut self, dropper: Dropper) {
        self.dropper = dropper;
    }

    pub fn set_decoder(&mut self, decoder: Decoder) {
        self.decoder = decoder;
    }

    pub fn get_encoder(&self) -> &Encoder {
        &self.encoder
    }

    pub fn get_dropper(&self) -> &Dropper {
        &self.dropper
    }

    pub fn get_decoder(&self) -> &Decoder {
        &self.decoder
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    use crate::drop::constant::ConstantDropScheduler;
    use crate::drop::ge::GilbertEliotDropSheduler;
    use crate::drop::specific::SpecificDropScheduler;
    use crate::drop::uniform::UniformDropScheduler;
    use crate::fec::maelstrom::{MaelstromDecoder, MaelstromEncoder};
    use crate::fec::tart::{AdaptiveFecScheduler, TartDecoder, TartEncoder, WindowStepScheduler};
    use crate::fec::FecDecoder;
    use crate::node::decoder::{Decoder, DecoderFeedback};
    use crate::node::dropper::Dropper;
    use crate::node::encoder::Encoder;
    use crate::Simulator;

    #[test]
    fn test_sim_no_nodes() {
        let mut simulator = Simulator::new();
        assert_eq!(simulator.run(100), Ok(()));

        assert_eq!(simulator.get_sink().get_lost(100), Vec::new());
        assert_eq!(simulator.get_sink().get_recovered(), Vec::new());
    }

    #[test]
    fn test_simple_tart_recovery() {
        let fec_max_wnd = 100;
        let fec_step = 5;
        let feedback_frequency = 500;
        let mut simulator = Simulator::new();

        // Add TART encoder with a WindowStepScheduler.
        let scheduler = WindowStepScheduler::new(fec_max_wnd, fec_step);
        let tart_encoder = TartEncoder::new(Box::new(scheduler), fec_max_wnd);
        let encoder = Encoder::new(crate::fec::FecEncoder::Tart(tart_encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let drop_scheduler = UniformDropScheduler::new(0.1, 1);
        let dropper = Dropper::new(Box::new(drop_scheduler));
        simulator.set_dropper(dropper);

        // Add TART decoder.
        let fec_decoder = FecDecoder::Tart(TartDecoder::new(fec_max_wnd));
        let feedback = DecoderFeedback::new(feedback_frequency);
        let decoder = Decoder::new(fec_decoder, Some(feedback));
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(100), Ok(()));

        let encoder = simulator.get_encoder();
        assert_eq!(encoder.get_nb_rs(), 20);
        assert_eq!(encoder.get_nb_ss(), 100);

        let dropper = simulator.get_dropper();
        assert_eq!(dropper.get_nb_dropped(), 11);

        let decoder = simulator.get_decoder();
        assert_eq!(decoder.get_nb_recovered(), 11);

        let sink = simulator.get_sink();
        assert_eq!(sink.get_lost(100), Vec::new());
        let recovered = sink.get_recovered();
        assert_eq!(recovered.len(), 11);
    }

    #[test]
    fn test_adaptive_tart_with_specific() {
        let fec_max_wnd = 100;
        let feedback_frequency = 500;
        let mut simulator = Simulator::new();

        // Add TART encoder with an adaptive scheduler.
        let mut scheduler = AdaptiveFecScheduler::new(0.5, fec_max_wnd);
        scheduler.set_initial_loss_estimation(0.2);
        let tart_encoder = TartEncoder::new(Box::new(scheduler), fec_max_wnd);
        let encoder = Encoder::new(crate::fec::FecEncoder::Tart(tart_encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let drop_scheduler = UniformDropScheduler::new(0.1, 1);
        let dropper = Dropper::new(Box::new(drop_scheduler));
        simulator.set_dropper(dropper);

        // Add TART decoder.
        let fec_decoder = FecDecoder::Tart(TartDecoder::new(fec_max_wnd));
        let feedback = DecoderFeedback::new(feedback_frequency);
        let decoder = Decoder::new(fec_decoder, Some(feedback));
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(100), Ok(()));

        let encoder = simulator.get_encoder();
        assert_eq!(encoder.get_nb_rs(), 20);
        assert_eq!(encoder.get_nb_ss(), 100);

        let dropper = simulator.get_dropper();
        assert_eq!(dropper.get_nb_dropped(), 11);

        let decoder = simulator.get_decoder();
        assert_eq!(decoder.get_nb_recovered(), 11);

        let sink = simulator.get_sink();
        assert_eq!(sink.get_lost(100), Vec::new());
        let recovered = sink.get_recovered();
        assert_eq!(recovered.len(), 11);
    }

    #[test]
    fn test_maelstrom() {
        let mut simulator = Simulator::new();
        let window = 8;
        let interleaves_values = vec![1, 4, 8];

        // Add encoder.
        let encoder = MaelstromEncoder::new(window, &interleaves_values);
        let encoder = Encoder::new(crate::fec::FecEncoder::Maelstrom(encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let drop_scheduler = ConstantDropScheduler::new(20);
        let dropper = Dropper::new(Box::new(drop_scheduler));
        simulator.set_dropper(dropper);

        // Add decoder.
        let decoder = MaelstromDecoder::new(window * 20);
        let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(100), Ok(()));

        assert_eq!(
            simulator.get_sink().get_recovered().len(),
            simulator.get_dropper().get_nb_ss_dropped() as usize
        );
        assert!(!simulator.get_sink().get_recovered().is_empty());
    }

    #[test]
    fn test_maelstrom_burst_two() {
        let mut simulator = Simulator::new();
        let window = 5;
        let interleaves_values = vec![1, 2];

        // Add encoder.
        let encoder = MaelstromEncoder::new(window, &interleaves_values);
        let encoder = Encoder::new(crate::fec::FecEncoder::Maelstrom(encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let mut drop_scheduler = SpecificDropScheduler::new(100);
        drop_scheduler.add_to_drop(&[3, 4, 5, 6]); // Drop ID 5 but a repair is sent in between.
        let mut dropper = Dropper::new(Box::new(drop_scheduler));
        dropper.activate_trace();
        simulator.set_dropper(dropper);

        // Add decoder.
        let decoder = MaelstromDecoder::new(window * 20);
        let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(10), Ok(()));
        assert_eq!(
            simulator.get_sink().get_recovered().len(),
            simulator.get_dropper().get_nb_ss_dropped() as usize
        );
        assert!(!simulator.get_sink().get_recovered().is_empty());
        let mut recovered = simulator.get_sink().get_recovered();
        recovered.sort();
        assert_eq!(recovered, vec![3, 4, 5]);
    }

    #[test]
    fn test_maelstrom_burst_three() {
        let mut simulator = Simulator::new();
        let window = 10;
        let interleaves_values = vec![1, 3];

        // Add encoder.
        let encoder = MaelstromEncoder::new(window, &interleaves_values);
        let encoder = Encoder::new(crate::fec::FecEncoder::Maelstrom(encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let mut drop_scheduler = SpecificDropScheduler::new(30);
        drop_scheduler.add_to_drop(&[3, 4, 5]); // Drop ID 5 but a repair is sent in between.
        let mut dropper = Dropper::new(Box::new(drop_scheduler));
        dropper.activate_trace();
        simulator.set_dropper(dropper);

        // Add decoder.
        let decoder = MaelstromDecoder::new(window * 20);
        let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(29), Ok(()));
        let mut recovered = simulator.get_sink().get_recovered();
        recovered.sort();
        assert_eq!(
            simulator.get_sink().get_recovered().len(),
            simulator.get_dropper().get_nb_ss_dropped() as usize
        );
        assert!(!simulator.get_sink().get_recovered().is_empty());
    }

    #[test]
    fn test_maelstrom_burst_ten() {
        let mut simulator = Simulator::new();
        let window = 10;
        let interleaves_values = vec![1, 10];

        // Add encoder.
        let encoder = MaelstromEncoder::new(window, &interleaves_values);
        let encoder = Encoder::new(crate::fec::FecEncoder::Maelstrom(encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let mut drop_scheduler = SpecificDropScheduler::new(100000); // Do not repeat
        drop_scheduler.add_to_drop(&(0..11).map(|i| i + 4).collect::<Vec<_>>()); // Drop ID 5 but a repair is sent in between.
        let mut dropper = Dropper::new(Box::new(drop_scheduler));
        dropper.activate_trace();
        simulator.set_dropper(dropper);

        // Add decoder.
        let decoder = MaelstromDecoder::new(window * 20);
        let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(100), Ok(()));
        let mut recovered = simulator.get_sink().get_recovered();
        recovered.sort();
        assert_eq!(
            simulator.get_sink().get_recovered().len(),
            simulator.get_dropper().get_nb_ss_dropped() as usize
        );
        assert!(!simulator.get_sink().get_recovered().is_empty());
    }

    #[test]
    fn test_maelstrom_burst_ge() {
        let mut simulator = Simulator::new();
        let window = 10;
        let interleaves_values = vec![1, 10];

        // Add encoder.
        let encoder = MaelstromEncoder::new(window, &interleaves_values);
        let encoder = Encoder::new(crate::fec::FecEncoder::Maelstrom(encoder));
        simulator.set_encoder(encoder);

        // Add dropper.
        let drop_scheduler = GilbertEliotDropSheduler::new_simple(0.01, 0.2, 1);
        let mut dropper = Dropper::new(Box::new(drop_scheduler));
        dropper.activate_trace();
        simulator.set_dropper(dropper);

        // Add decoder.
        let decoder = MaelstromDecoder::new(window * 20);
        let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);
        simulator.set_decoder(decoder);

        assert_eq!(simulator.run(1000), Ok(()));

        println!("Nb generated repairs: {:?}", simulator.get_encoder().get_nb_rs());
        let mut recovered = simulator.get_sink().get_recovered();
        recovered.sort();
        assert_eq!(
            simulator.get_sink().get_recovered().len(),
            simulator.get_dropper().get_nb_ss_dropped() as usize
        );
        assert!(!simulator.get_sink().get_recovered().is_empty());
    }

    
}

pub mod drop;
pub mod fec;
pub mod node;

use fec::FecMetadata;
use node::{decoder::Decoder, dropper::Dropper, encoder::Encoder, Node, Sink, Source};
