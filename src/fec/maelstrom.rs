use std::collections::HashMap;
use std::collections::HashSet;

use crate::Error;
use crate::Packet;
use crate::Result;
use std::fmt::Debug;

use super::FecMetadata;
use super::FecRepairMetadata;
use super::FecSourceMetadata;

pub type MaelstromSSID = u64;

#[derive(Clone, Debug)]
/// Maelstrom repair FEC information.
pub struct MaelstromRepairInfo {
    /// List of source symbols protected by this repair symbol.
    ssid: Vec<MaelstromSSID>,
}

pub struct MaelstromEncoder {
    /// Current source symbol ID.
    ssid: u64,

    /// Interleaves. Implicitly sets the number of bins for each interleave layer.
    /// E.g., an interleave of 100 means that 100 bins are used.
    interleaves: Vec<Vec<Bin>>,

    /// All packets that are protected and still in scope.
    pkts: HashSet<Packet>,

    /// Maximum number of source symbols.
    max_wnd: usize,
}

impl Debug for MaelstromEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "maelstrom_{}_{}",
            self.interleaves
                .iter()
                .map(|layer| format!("{}", layer.len()))
                .collect::<Vec<String>>()
                .join("_"),
            self.max_wnd,
        )
    }
}

impl MaelstromEncoder {
    pub fn new(window: usize, interleaves_values: &[u64]) -> Self {
        let mut interleaves = Vec::with_capacity(interleaves_values.len());
        for interleave in interleaves_values.iter() {
            let bins = (0..*interleave).map(|_| Bin::new(window)).collect();
            interleaves.push(bins);
        }
        Self {
            ssid: 0,
            interleaves,
            pkts: HashSet::new(),
            max_wnd: window,
        }
    }

    /// Protects a new packet.
    pub fn protect_symbol(&mut self, pkt: &mut Packet) -> Result<()> {
        pkt.add_fec_metadata(super::FecMetadata::Source(FecSourceMetadata::Maelstrom(
            self.ssid,
        )))?;

        // Remove old packets from the window.
        let id_to_remove = self.ssid.saturating_sub(self.max_wnd as u64);
        // Remove expired packets from the hashmap using SSID.
        self.pkts = self
            .pkts
            .drain()
            .filter(|pkt| pkt.id > id_to_remove)
            .collect();

        // Add the packet to the list of packets that are protected.
        self.pkts.insert(pkt.clone());

        // Add the packet to the correct bin of each layer.
        self.interleaves.iter_mut().for_each(|layer| {
            let n = layer.len();
            let bin = &mut layer[self.ssid as usize % n];
            bin.symbols.insert(self.ssid);
        });

        self.ssid += 1;
        Ok(())
    }

    /// Whether at least a bin from a layer should generate a repair symbol.
    pub fn should_generate_rs(&self) -> bool {
        self.interleaves
            .iter()
            .any(|layer| layer.iter().any(|bin| bin.should_generate_rs()))
    }

    /// Generate as many repair symbols as needed by calling every bin from every layer.
    pub fn generate_rs(&mut self) -> Result<Vec<Packet>> {
        Ok(self
            .interleaves
            .iter_mut()
            .flat_map(|layer| {
                layer
                    .iter_mut()
                    .filter_map(|bin| bin.generate_rs(&self.pkts))
            })
            .collect())
    }

    /// Get the total number of repair symbols generated.
    pub fn get_nb_rs(&self) -> u64 {
        self.interleaves
            .iter()
            .flat_map(|layer| layer.iter().map(|bin| bin.get_nb_rs()))
            .sum()
    }
}

/// A bin of an interleave. Contains the symbols to protect and materials to generate the repair symbols.
struct Bin {
    /// Stored source symbols that will be protected.
    symbols: HashSet<MaelstromSSID>,

    /// Number of repair symbols generated.
    nb_rs: u64,

    /// Maximum size of the window, i.e., number of source symbols required to generate a repair symbol.
    window_size: usize,
}

impl Bin {
    fn new(window_size: usize) -> Self {
        Self {
            symbols: HashSet::new(),
            nb_rs: 0,
            window_size,
        }
    }

    fn generate_rs(&mut self, all_pkts: &HashSet<Packet>) -> Option<Packet> {
        if self.symbols.len() >= self.window_size {
            let mut pkt = all_pkts
                .iter()
                .filter(|pkt| self.symbols.contains(&pkt.id))
                .xor();

            // Add FEC repair state for the generated repair packet.
            let repair_info = MaelstromRepairInfo {
                ssid: self.symbols.iter().copied().collect(),
            };
            pkt.add_fec_metadata(super::FecMetadata::Repair(
                super::FecRepairMetadata::Maelstrom(repair_info),
            ))
            .ok();

            // Reset state.
            self.symbols = HashSet::new();
            self.nb_rs += 1;

            Some(pkt)
        } else {
            None
        }
    }

    fn should_generate_rs(&self) -> bool {
        self.symbols.len() >= self.window_size
    }

    fn get_nb_rs(&self) -> u64 {
        self.nb_rs
    }
}

pub trait XorPackets {
    fn xor(self) -> Packet;
}

impl<'a, I> XorPackets for I
where
    I: Iterator<Item = &'a Packet>,
{
    fn xor(self) -> Packet {
        let data = self.fold(0, |cur, pkt| {
            cur ^ u64::from_be_bytes(pkt.data.clone().try_into().unwrap())
        });

        Packet {
            id: data,
            fec: None,
            recovered: None,
            data: data.to_be_bytes().to_vec(),
        }
    }
}

pub struct MaelstromDecoder {
    /// All equations.
    equations: HashMap<u64, Equation>,

    /// Max SSID received. Used to prune too old packets.
    max_ssid: u64,

    /// Current equation ID.
    eq_id: u64,

    /// All source symbols received.
    pkts: HashMap<u64, Packet>,

    /// Maximum number of source symbols stored.
    capacity: usize,
}

impl MaelstromDecoder {
    pub fn new(capacity: usize) -> Self {
        Self {
            equations: HashMap::new(),
            max_ssid: 0,
            eq_id: 0,
            pkts: HashMap::new(),
            capacity,
        }
    }

    pub fn recv_ss(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        if let Some(FecMetadata::Source(FecSourceMetadata::Maelstrom(mut metadata))) = pkt.fec {
            let id_to_remove = metadata.saturating_sub(self.capacity as u64);
            let _ids_to_remove: Vec<_> = self
                .equations
                .values()
                .filter(|eq| eq.get_min_ssid().unwrap() >= id_to_remove)
                .map(|eq| eq.id)
                .collect();
            // for idx in ids_to_remove {
            //     self.equations.remove(&idx);
            // }

            // Remove expired packets from the hashmap using SSID.
            // self.pkts = self
            //     .pkts
            //     .drain()
            //     .filter(|(id, _)| *id > id_to_remove)
            //     .collect();

            // Add to the list of received packets.
            self.pkts.insert(pkt.id, pkt.clone());
            self.max_ssid = self.max_ssid.max(metadata);

            // Add the ID to the existing equations. No effect on equations that did not need it.
            let mut ids_to_remove = HashSet::new();
            let mut recovered = HashSet::new();
            loop {
                // Add the symbol to all equations.
                for equation in self.equations.values_mut() {
                    if equation.add_symbol(metadata) == DecoderAction::Redundant {
                        ids_to_remove.insert(equation.id);
                    }
                }

                // Solve an equation thanks to this symbol. Restart until no new source symbol can be recovered.
                let mut at_least_one = false;
                for equation in self.equations.values_mut() {
                    if equation.action() == DecoderAction::Recover
                        && !ids_to_remove.contains(&equation.id)
                    {
                        let local = equation.recover(&self.pkts);
                        if let Some(mut rec) = local {
                            rec.recovered = Some(pkt.id.saturating_sub(rec.id));
                            metadata = u64::from_be_bytes(rec.data.clone().try_into().unwrap());
                            recovered.insert(rec.clone());
                            at_least_one = true;
                            self.pkts.insert(rec.id, rec.clone());
                            // println!("Recover source symbol: {} for equation {} but received is: {}", metadata, equation.id, pkt.id);
                            break;
                        }
                    }
                }

                if !at_least_one {
                    break;
                }
            }

            // Clean expired equations.
            for id in ids_to_remove {
                self.equations.remove(&id);
            }

            Ok(recovered.into_iter().collect())
        } else {
            Err(Error::FecWrongMetadata)
        }
    }

    pub fn recv_rs(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        if let Some(FecMetadata::Repair(FecRepairMetadata::Maelstrom(repair))) = pkt.fec.as_ref() {
            let mut recovered = HashSet::new();

            // Maybe the equation is too old (i.e., source symbols are already removes from the window).
            // In that case, we do not use the equation.
            if *repair.ssid.iter().min().ok_or(Error::FecWrongMetadata)?
                < self.max_ssid.saturating_sub(self.capacity as u64)
            {
                // error!("Too old equation. Do not proceed.");
                return Err(Error::TooOldEquation);
            }

            // Add a new equation from this repair symbol.
            let mut new_equation = Equation::new(pkt.clone(), self.eq_id)?;
            self.eq_id += 1;
            new_equation.populate(&self.pkts);
            match new_equation.action() {
                DecoderAction::Redundant => (), // Useless repair symbol.
                DecoderAction::Missing => {
                    // Not enough source symbols to recover a packet.
                    // This does not change the state of the other equations as well.
                    self.equations.insert(self.eq_id, new_equation);
                }
                DecoderAction::Recover => {
                    // Resolve the equation but do not add it to the system because we solve it directly.
                    let local = new_equation.recover(&self.pkts);
                    if let Some(mut rec) = local {
                        rec.recovered = Some(pkt.id.saturating_sub(rec.id));
                        recovered.extend(self.recv_ss(&rec)?);
                        recovered.insert(rec);
                    }
                }
            }
            Ok(recovered.into_iter().collect())
        } else {
            Err(Error::FecWrongMetadata)
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
enum DecoderAction {
    /// Missing source symbols.
    Missing,

    /// Able to recover a lost source symbol.
    Recover,

    /// All soure symbols have been received. The equation can be deleted.
    Redundant,
}

#[derive(Debug)]
pub struct Equation {
    /// IDs of source symbols protected by this equation that are received.
    recv_ssid: HashSet<MaelstromSSID>,

    /// Repair FEC payload
    repair: Packet,

    /// IDs of source symbols that are needed by this equation.
    need_ssid: HashSet<MaelstromSSID>,

    /// Unique ID.
    id: u64,
}

impl Equation {
    fn new(repair: Packet, id: u64) -> Result<Self> {
        if let Some(FecMetadata::Repair(FecRepairMetadata::Maelstrom(fec))) = repair.fec.clone() {
            Ok(Self {
                recv_ssid: HashSet::new(),
                need_ssid: fec.ssid.iter().copied().collect(),
                repair,
                id,
            })
        } else {
            Err(Error::FecWrongMetadata)
        }
    }

    /// Fill all received source symbols in the equation. Returns true if all symbols have been received.
    fn populate(&mut self, pkts: &HashMap<u64, Packet>) -> DecoderAction {
        pkts.values().for_each(|pkt| {
            if self.need_ssid.contains(&pkt.id) {
                self.recv_ssid.insert(pkt.id);
            }
        });
        self.action()
    }

    fn action(&self) -> DecoderAction {
        match self.need_ssid.len().saturating_sub(self.recv_ssid.len()) as u64 {
            0 => DecoderAction::Redundant,
            1 => DecoderAction::Recover,
            _ => DecoderAction::Missing,
        }
    }

    /// Add a new source symbol to the bin. Returns true if the equation can be solved.
    /// Returns false otherwise. Also returns false if the symbol was not needed by this equation.
    fn add_symbol(&mut self, id: MaelstromSSID) -> DecoderAction {
        if self.need_ssid.contains(&id) {
            self.recv_ssid.insert(id);
        }
        self.action()
    }

    /// Recover a lost source symbol.
    fn recover(&mut self, pkts: &HashMap<u64, Packet>) -> Option<Packet> {
        if self.action() == DecoderAction::Recover {
            let mut rec = pkts
                .values()
                .filter(|pkt| self.need_ssid.contains(&pkt.id))
                .chain([&self.repair].iter().copied())
                .xor();
            // Add FEC source symbol ID to the packet.
            let ssid = self.need_ssid.difference(&self.recv_ssid).next().unwrap();
            rec.fec = Some(FecMetadata::Source(FecSourceMetadata::Maelstrom(*ssid)));
            rec.id = *ssid;
            // Do not forget to say that we do not need the equation anymore!
            self.recv_ssid.insert(*ssid);
            Some(rec)
        } else {
            None
        }
    }

    /// Returns the minimum SSID for this equation.
    fn get_min_ssid(&self) -> Option<MaelstromSSID> {
        self.need_ssid.iter().min().copied()
    }
}
