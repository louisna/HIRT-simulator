use crate::Result;
use crate::Error;
use crate::{fec::FecEncoder, node::Node, Packet};

/// Encoder structure.
pub struct Encoder {
    /// Number of data packets received.
    nb_ss: u64,

    /// Number of repair packets generated.
    nb_rs: u64,

    /// (Ordered) pool of received packets that need to be processed.
    pkts: Vec<Packet>,

    /// FEC algorithm for the encoder.
    fec: FecEncoder,
}

impl Node for Encoder {
    fn recv(&mut self, pkts: Vec<Packet>) -> Result<()> {
        self.pkts.extend(pkts);
        Ok(())
    }

    fn forw(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::with_capacity(self.pkts.len());
        self.nb_ss += self.pkts.len() as u64;
        for mut pkt in self.pkts.drain(0..self.pkts.len()) {
            self.fec.protect_symbol(&mut pkt)?;
            out.push(pkt);
            if self.fec.should_generate_rs() {
                let repairs = match self.fec.generate_rs() {
                    Ok(v) => v,
                    Err(Error::FecEncoder(e)) if e == "NoSymbolToGenerate".to_string() => Vec::new(),
                    Err(e) => return Err(e),
                };
                self.nb_rs += repairs.len() as u64;
                out.extend(repairs);
            }
        }

        Ok(out)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Encoder {
    pub fn new(fec: FecEncoder) -> Self {
        Self {
            nb_ss: 0,
            nb_rs: 0,
            pkts: Vec::new(),
            fec,
        }
    }

    pub fn new_simple() -> Self {
        Self {
            nb_ss: 0,
            nb_rs: 0,
            pkts: Vec::new(),
            fec: FecEncoder::None,
        }
    }

    pub fn get_nb_rs(&self) -> u64 {
        self.nb_rs
    }

    pub fn get_nb_ss(&self) -> u64 {
        self.nb_ss
    }

    pub fn recv_feedback(&mut self, feedback: Vec<(u64, u64)>) {
        for (nb_lost, nb_elems) in feedback {
            self.fec.recv_feedback(nb_lost, nb_elems);
        }
    }

    pub fn get_fec_encoder(&self) -> &FecEncoder {
        &self.fec
    }
}
