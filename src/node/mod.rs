use std::any::Any;
use std::collections::HashSet;

use crate::Packet;
use crate::Result;
pub mod decoder;
pub mod dropper;
pub mod encoder;

/// A node that receives and forwards packets.
pub trait Node {
    /// Receives packets in its internal buffer.
    fn recv(&mut self, pkts: Vec<Packet>) -> Result<()>;

    /// Forwards packets. The node may send more/fewer packets than received.
    fn forw(&mut self) -> Result<Vec<Packet>>;

    fn as_any(&self) -> &dyn Any;
}

/// A node that generates packets.
pub struct Source {
    /// ID of the next packet to generate.
    id: u64,
}

impl Source {
    /// Generates a new packet.
    pub fn gen(&mut self) -> Packet {
        let pkt = Packet::new(self.id);
        self.id += 1;
        pkt
    }

    pub fn new() -> Self {
        Self { id: 0 }
    }
}

/// A node that receives packets.
pub struct Sink {
    /// Received packets. Store in a vector to see duplicates.
    recv: Vec<Packet>,
}

impl Sink {
    pub fn new() -> Self {
        Self { recv: Vec::new() }
    }
    /// Receives a packet.
    pub fn recv(&mut self, pkt: Packet) {
        self.recv.push(pkt);
    }

    /// Receives multiple packets.
    pub fn recv_multiple(&mut self, pkt: Vec<Packet>) {
        self.recv.extend(pkt);
    }

    /// Returns the list of packet IDs that were recovered.
    pub fn get_recovered(&self) -> Vec<u64> {
        self.recv
            .iter()
            .filter(|pkt| pkt.recovered.is_some())
            .map(|pkt| pkt.id)
            .collect()
    }

    /// Returns a list of packet lateness for recovered packets.
    /// For each recovered packet, the value means the number of source symbols arriving at the decoder before beeing able to recover it.
    pub fn get_recovering_delay(&self) -> Vec<(u64, u64)> {
        self.recv.iter().filter(|pkt| pkt.recovered.is_some()).map(|pkt| (pkt.id, pkt.recovered.unwrap())).collect()
    }

    /// Returns the list of packet IDs that are lost.
    pub fn get_lost(&self, max_id: u64) -> Vec<u64> {
        let recv: HashSet<u64> = self.recv.iter().map(|pkt| pkt.id).collect();
        (0..max_id).filter(|i| !recv.contains(i)).collect()
    }

    /// Returns the number of duplicates
    pub fn get_duplicates(&self) -> Vec<u64> {
        let mut out = Vec::new();
        let mut uniques = HashSet::new();
        for pkt in self.recv.iter() {
            if uniques.contains(&pkt.id) {
                out.push(pkt.id);
            } else {
                uniques.insert(pkt.id);
            }
        }

        out
    }
}
