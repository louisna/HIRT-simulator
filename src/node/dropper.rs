use crate::drop::none::NoDropScheduler;
use crate::drop::DropScheduler;
use crate::fec::FecMetadata;
use crate::node::Node;
use crate::node::Packet;
use crate::Result;

pub type DropTrace = (u64, bool, bool);

/// Dropper structure.
pub struct Dropper {
    scheduler: Box<dyn DropScheduler>,

    nb_recv: u64,

    nb_drop: u64,

    nb_drop_ss: u64,

    pkts: Vec<Packet>,

    trace: Option<Vec<DropTrace>>,
}

impl Node for Dropper {
    fn recv(&mut self, pkts: Vec<Packet>) -> Result<()> {
        self.nb_recv += pkts.len() as u64;
        self.pkts.extend(pkts);
        Ok(())
    }

    fn forw(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::with_capacity(self.pkts.len());
        for pkt in self.pkts.drain(0..self.pkts.len()) {
            let is_repair = matches!(pkt.fec, Some(FecMetadata::Repair(_)));
            let id = pkt.id;

            let is_dropped = if self.scheduler.should_drop() {
                self.nb_drop += 1;

                if let Some(FecMetadata::Source(_)) = pkt.fec {
                    self.nb_drop_ss += 1;
                }

                true
            } else {
                out.push(pkt);
                false
            };

            if let Some(trace) = self.trace.as_mut() {
                trace.push((id, is_repair, is_dropped));
            }
        }
        Ok(out)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Dropper {
    pub fn new(scheduler: Box<dyn DropScheduler>) -> Self {
        Self {
            scheduler,
            nb_recv: 0,
            nb_drop: 0,
            nb_drop_ss: 0,
            pkts: Vec::new(),
            trace: None,
        }
    }

    pub fn new_simple() -> Self {
        Self {
            scheduler: Box::new(NoDropScheduler {}),
            nb_drop: 0,
            nb_drop_ss: 0,
            nb_recv: 0,
            pkts: Vec::new(),
            trace: None,
        }
    }

    pub fn get_nb_dropped(&self) -> u64 {
        self.nb_drop
    }

    pub fn get_nb_ss_dropped(&self) -> u64 {
        self.nb_drop_ss
    }

    pub fn get_nb_recv(&self) -> u64 {
        self.nb_recv
    }

    pub fn get_dropped_ratio_posteriori(&self) -> f64 {
        self.nb_drop as f64 / self.nb_recv as f64
    }

    pub fn activate_trace(&mut self) {
        self.trace = Some(Vec::new())
    }

    pub fn get_trace(&self) -> Option<&[DropTrace]> {
        self.trace.as_ref().map(|i| i.as_slice())
    }

    pub fn get_dropped_ss(&self) -> Option<Vec<u64>> {
        self.trace.as_ref().map(|v| {
            v.iter()
                .filter(|(_, is_repair, is_dropped)| !is_repair && *is_dropped)
                .map(|(id, _, _)| *id).collect()
        })
    }
}
