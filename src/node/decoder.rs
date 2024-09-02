use bitmaps::Bitmap;
use networkcoding::source_symbol_metadata_to_u64;

use crate::fec::FecMetadata;
use crate::{fec::FecDecoder, Packet};
use crate::{Error, Result};

/// Encoder structure.
pub struct Decoder {
    /// Number of data packets received.
    nb_ss: u64,

    /// Number of repair packets received.
    nb_rs: u64,

    /// Number of recovered symbols.
    nb_recovered: u64,

    /// (Ordered) pool of received packets that need to be processed.
    pkts: Vec<Packet>,

    /// FEC algorithm for the decoder.
    fec: FecDecoder,

    /// Feedback scheduler.
    feedback: Option<DecoderFeedback>,

    /// Trace recording all source symbols that have been recovered.
    trace: Option<Vec<u64>>,
}

impl Decoder {
    pub fn recv(&mut self, pkts: Vec<Packet>) -> Result<()> {
        self.pkts.extend(pkts);
        Ok(())
    }

    pub fn forw(&mut self) -> Result<(Vec<Packet>, Vec<(u64, u64)>)> {
        let mut out = Vec::with_capacity(self.pkts.len());
        let mut feedback_pkts = Vec::with_capacity(1);

        for mut pkt in self.pkts.drain(0..self.pkts.len()) {
            match pkt.fec {
                Some(FecMetadata::Source(_)) => {
                    self.nb_ss += 1;

                    // Add packet to FEC window.
                    match self.fec.recv_ss(&pkt) {
                        Ok(recovered) => {
                            if !recovered.is_empty() {
                                println!("Recovered packets from source symbol: {}", recovered.len());
                                self.nb_recovered += recovered.len() as u64;
                                if let Some(trace) = self.trace.as_mut() {
                                    trace.extend(recovered.iter().map(|p| p.id));
                                }
                                out.extend(recovered);
                            }
                        }
                        Err(e) => error!("Error while decoding source symbol: {:?}", e),
                    }

                    // Add packet to feedback.
                    if let Some(feedback) = self.feedback.as_mut() {
                        let id = source_symbol_metadata_to_u64(
                            pkt.data
                                .to_owned()
                                .try_into()
                                .map_err(|_| Error::FecWrongMetadata)?,
                        );
                        feedback.recv_ss(id)?;

                        if feedback.should_send_feedback(id) {
                            let total = feedback.nb_since_last(id);
                            let nb_lost = total.saturating_sub(feedback.nb_recv());
                            feedback_pkts.push((nb_lost, total));
                            feedback.reset(id);
                        }
                    }

                    // Remove FEC.
                    pkt.fec = None;

                    // Finally forward the packet.
                    out.push(pkt);
                }
                Some(FecMetadata::Repair(_)) => {
                    self.nb_rs += 1;

                    match self.fec.recv_rs(&pkt) {
                        Ok(recovered) => {
                            if !recovered.is_empty() {
                                self.nb_recovered += recovered.len() as u64;
                                if let Some(trace) = self.trace.as_mut() {
                                    trace.extend(recovered.iter().map(|p| p.id));
                                }
                                out.extend(recovered);
                            }
                        },
                        Err(Error::TooOldEquation) => debug!("Confirmed too old equation"),
                        Err(Error::UnusedRepair) => debug!("Unused repair symbol. Do nothing."),
                        Err(e) => error!("Error while decoding repair symbol: {e:?}"),
                    }

                    // Do not push the repair packet to the output.
                }
                None => out.push(pkt),
            }
        }
        Ok((out, feedback_pkts))
    }
}

impl Decoder {
    pub fn new(fec: FecDecoder, feedback: Option<DecoderFeedback>) -> Self {
        Self {
            nb_ss: 0,
            nb_rs: 0,
            nb_recovered: 0,
            pkts: Vec::new(),
            fec,
            feedback,
            trace: None,
        }
    }

    pub fn new_simple() -> Self {
        Self {
            nb_ss: 0,
            nb_rs: 0,
            nb_recovered: 0,
            pkts: Vec::new(),
            fec: FecDecoder::None,
            feedback: None,
            trace: None
        }
    }

    pub fn get_nb_recovered(&self) -> u64 {
        self.nb_recovered
    }

    pub fn activate_trace(&mut self) {
        self.trace = Some(Vec::new())
    }

    pub fn get_trace(&self) -> Option<&[u64]> {
        self.trace.as_ref().map(|t| t.as_slice())
    }
}

pub struct DecoderFeedback {
    /// Number of source symbols before sending feedback.
    frequency: u64,

    /// Last feedback SSID.
    last_feedback: u64,

    /// Bitmap of received source symbols in this feedback.
    bitmap: Bitmap<1024>,
}

impl DecoderFeedback {
    pub fn new(frequency: u64) -> Self {
        Self {
            frequency,
            last_feedback: 0,
            bitmap: Bitmap::new(),
        }
    }

    pub fn recv_ss(&mut self, id: u64) -> Result<()> {
        let relative_id = id - self.last_feedback;
        if relative_id > 1024 {
            return Err(Error::FeedbackIdTooBig);
        }

        self.bitmap.set(relative_id as usize, true);

        Ok(())
    }

    pub fn nb_recv(&self) -> u64 {
        self.bitmap.len() as u64
    }

    pub fn nb_since_last(&self, id: u64) -> u64 {
        id - self.last_feedback
    }

    pub fn reset(&mut self, id: u64) {
        self.last_feedback = id;
        self.bitmap = Bitmap::new();
    }

    pub fn should_send_feedback(&self, id: u64) -> bool {
        id - self.last_feedback >= self.frequency
    }
}
