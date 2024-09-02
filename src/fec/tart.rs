use super::FecRepairMetadata;
use super::FecSourceMetadata;
use crate::Error;
use crate::FecMetadata;
use crate::Packet;
use crate::Result;
#[cfg(feature = "rlc")]
use networkcoding::rlc::decoder::RLCDecoder;
#[cfg(feature = "rlc")]
use networkcoding::rlc::encoder::RLCEncoder;
use networkcoding::source_symbol_metadata_from_u64;
use networkcoding::source_symbol_metadata_to_u64;
use networkcoding::vandermonde_lc::decoder::VLCDecoder;
use networkcoding::vandermonde_lc::encoder::VLCEncoder;
use networkcoding::Decoder;
use networkcoding::Encoder;
use networkcoding::SourceSymbol;
use networkcoding::SourceSymbolMetadata;
use std::fmt::Debug;
use std::time::Instant;

const MAX_WINDOW_FACTOR: usize = 500;

pub struct TartEncoder {
    tart: Encoder,

    scheduler: Box<dyn TartFecScheduler>,

    max_wnd: usize,
}

impl TartEncoder {
    pub fn protect_symbol(&mut self, pkt: &mut Packet) -> Result<()> {
        let mut next_metadata = self.tart.next_metadata().unwrap();
        self.tart
            .protect_data(pkt.data.clone(), &mut next_metadata)
            .unwrap();
        pkt.add_fec_metadata(FecMetadata::Source(FecSourceMetadata::Tart(next_metadata)))?;
        if self.tart.n_protected_symbols() >= self.max_wnd {
            self.reset();
        }

        Ok(())
    }

    pub fn next_id(&mut self) -> u64 {
        source_symbol_metadata_to_u64(self.tart.next_metadata().unwrap())
    }

    pub fn should_send_rs(&mut self) -> bool {
        let next_id = self.next_id();
        self.scheduler.should_generate_rs(next_id)
    }

    pub fn on_sent_rs(&mut self) {
        let next_id = self.next_id();
        self.scheduler.on_sent_rs(next_id)
    }

    pub fn reset(&mut self) {
        let next_id = self.next_id();
        let id_to_remove = next_id.saturating_sub(self.max_wnd as u64);
        if id_to_remove > 0 {
            self.tart
                .remove_up_to(source_symbol_metadata_from_u64(id_to_remove));
        }
    }

    pub fn generate_rs(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::new();

        let current_id = self.next_id();
        while self.scheduler.should_generate_rs(current_id) {
            let repair = self
                .tart
                .generate_and_serialize_repair_symbol()
                .map_err(|e| Error::FecEncoder(format!("{:?}", e).to_string()))?;
            let rs = Packet {
                id: current_id,
                fec: Some(FecMetadata::Repair(FecRepairMetadata::Tart(repair))),
                recovered: None,
                data: Vec::new(),
            };
            out.push(rs);
            self.on_sent_rs();
        }

        Ok(out)
    }

    pub fn new(scheduler: Box<dyn TartFecScheduler>, max_wnd: u64) -> Self {
        Self {
            #[cfg(feature = "rlc")]
            tart: Encoder::RLC(RLCEncoder::new(8, max_wnd as usize * 10, 1)),
            #[cfg(not(feature = "rlc"))]
            tart: Encoder::VLC(VLCEncoder::new(8, max_wnd as usize * MAX_WINDOW_FACTOR)),
            scheduler,
            max_wnd: max_wnd as usize,
        }
    }

    pub fn recv_feedback(&mut self, nb_lost: u64, nb_elems: u64) {
        self.scheduler.recv_feedback(nb_lost, nb_elems);
    }
}

impl Debug for TartEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tart_{:?}", self.scheduler)
    }
}

pub trait TartFecScheduler: Debug {
    fn should_generate_rs(&self, current: u64) -> bool;

    fn on_sent_rs(&mut self, current: u64);

    fn should_reset_up_to(&mut self, current: u64) -> SourceSymbolMetadata;

    fn recv_feedback(&mut self, nb_lost: u64, nb_elems: u64);
}

pub struct TartDecoder {
    tart: Decoder,

    max_window: u64,
}

impl TartDecoder {
    pub fn recv_ss(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        if let Some(FecMetadata::Source(FecSourceMetadata::Tart(metadata))) = pkt.fec {
            let id = source_symbol_metadata_to_u64(metadata);
            let id_to_remove = id.saturating_sub(self.max_window * 2);
            if id_to_remove > 0 {
                // self.tart
                //     .remove_up_to(source_symbol_metadata_from_u64(id_to_remove), None);
            }

            let source_symbol = SourceSymbol::new(metadata, pkt.data.clone());
            match self
                .tart
                .receive_source_symbol(source_symbol, Instant::now())
            {
                Err(e) => Err(Error::FecDecoder(format!("{:?}", e).to_string())),
                Ok(decoded_symbols) => Ok(decoded_symbols
                    .iter()
                    .map(|symbol| {
                        Packet::new_recovered(
                            u64::from_be_bytes(symbol.get().to_owned().try_into().unwrap()),
                            pkt.id,
                        )
                    })
                    .collect()),
            }
        } else {
            Err(Error::FecWrongMetadata)
        }
    }

    pub fn recv_rs(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        if let Some(FecMetadata::Repair(FecRepairMetadata::Tart(repair_symbol))) = &pkt.fec {
            match self
                .tart
                .receive_and_deserialize_repair_symbol(repair_symbol.to_owned())
                .map(|(_, recovered_symbols)| {
                    recovered_symbols
                        .iter()
                        .map(|symbol| {
                            Packet::new_recovered(
                                u64::from_be_bytes(symbol.get().to_owned().try_into().unwrap()),
                                pkt.id,
                            )
                        })
                        .collect()
                }) {
                Ok(v) => Ok(v),
                Err(networkcoding::DecoderError::UnusedRepairSymbol) => Err(Error::UnusedRepair),
                Err(e) => Err(Error::FecDecoder(format!("{:?}", e).to_string())),
            }
        } else {
            Err(Error::FecWrongMetadata)
        }
    }

    pub fn new(max_wnd: u64) -> Self {
        Self {
            #[cfg(feature = "rlc")]
            tart: Decoder::RLC(RLCDecoder::new(8, max_wnd as usize * 10)),
            #[cfg(not(feature = "rlc"))]
            tart: Decoder::VLC(VLCDecoder::new(8, max_wnd as usize * MAX_WINDOW_FACTOR)),
            max_window: max_wnd,
        }
    }
}

pub struct WindowStepScheduler {
    /// Maximum number of symbols in the window.
    max_wnd: u64,

    /// Step between two repair symbols.
    step: u64,

    /// Last sent repair symbol.
    last_sent: u64,
}

impl TartFecScheduler for WindowStepScheduler {
    fn should_generate_rs(&self, current: u64) -> bool {
        current.saturating_sub(self.last_sent) >= self.step
    }

    fn on_sent_rs(&mut self, current: u64) {
        self.last_sent = current;
    }

    fn should_reset_up_to(&mut self, current: u64) -> SourceSymbolMetadata {
        source_symbol_metadata_from_u64(current.saturating_sub(self.max_wnd))
    }

    fn recv_feedback(&mut self, _nb_lost: u64, _nb_elems: u64) {}
}

impl Debug for WindowStepScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window_{step}", step = self.step)
    }
}

impl WindowStepScheduler {
    pub fn new(max_wnd: u64, step: u64) -> Self {
        Self {
            max_wnd,
            step,
            last_sent: 0,
        }
    }
}

pub struct AdaptiveFecScheduler {
    /// Estimated mean loss percentage based on feedback.
    loss_estimation: f64,

    /// Variance of the loss estimation.
    loss_variance_estimation: f64,

    /// Learning parameter for the moving average.
    alpha: f64,

    /// Tweaking parameter to increase redundancy ratio.
    beta: f64,

    /// SSID where the last repair symbol was sent.
    last_sent_ssid: u64,

    /// Maximum window size
    wsize: u64,
}

impl Debug for AdaptiveFecScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "adaptive_{}_{}_{}", self.alpha, self.beta, self.wsize)
    }
}

impl TartFecScheduler for AdaptiveFecScheduler {
    fn should_generate_rs(&self, current: u64) -> bool {
        // Generate enough repair symbols to alleviate the loss percentage estimated by the feedback.
        // Spread these repair symbols.
        if self.loss_estimation == 0.0 {
            return false;
        }

        let nb_lost_pkt_per_window = (self.loss_estimation
            + self.beta * self.loss_variance_estimation)
            * self.wsize as f64
            * self.beta;

        let next_rs = self.wsize as f64 / nb_lost_pkt_per_window;
        current.saturating_sub(self.last_sent_ssid) as f64 >= next_rs
    }

    fn on_sent_rs(&mut self, current: u64) {
        self.last_sent_ssid = current;
    }

    fn should_reset_up_to(&mut self, current: u64) -> SourceSymbolMetadata {
        source_symbol_metadata_from_u64(current.saturating_sub(self.wsize))
    }

    fn recv_feedback(&mut self, nb_lost: u64, nb_elems: u64) {
        if nb_elems == 0 {
            return;
        }
        let local_loss = nb_lost as f64 / nb_elems as f64;
        let local_variance = (self.loss_estimation - local_loss).abs();
        self.loss_estimation = self.loss_estimation * self.alpha + (1.0 - self.alpha) * local_loss;
        self.loss_variance_estimation =
            self.loss_variance_estimation * self.alpha + (1.0 - self.alpha) * local_variance;
        info!(
            "New loss estimation: {} from local loss estimation: {}",
            self.loss_estimation, local_loss
        );
        info!(
            "New variance loss estimation: {} from local variance loss estimation: {}",
            self.loss_variance_estimation, local_variance
        );
    }
}

impl AdaptiveFecScheduler {
    pub fn new(alpha: f64, wsize: u64) -> Self {
        Self {
            loss_estimation: 0.0,
            loss_variance_estimation: 0.0,
            alpha,
            last_sent_ssid: 0,
            wsize,
            beta: 1.0,
        }
    }

    pub fn set_initial_loss_estimation(&mut self, loss: f64) {
        self.loss_estimation = loss;
    }

    pub fn set_beta_fec(&mut self, beta: f64) {
        self.beta = beta;
    }

    pub fn set_alpha_fec(&mut self, alpha: f64) {
        self.alpha = alpha;
    }
}
