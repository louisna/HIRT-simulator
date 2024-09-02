#[macro_use]
extern crate log;

use std::fs;

use clap::Parser;
use fec_simulator::drop::constant::ConstantDropScheduler;
use fec_simulator::drop::ge::GilbertEliotDropSheduler;
use fec_simulator::drop::none::NoDropScheduler;
use fec_simulator::drop::specific::SpecificDropScheduler;
use fec_simulator::drop::uniform::UniformDropScheduler;
use fec_simulator::drop::DropScheduler;
use fec_simulator::fec::maelstrom::{MaelstromDecoder, MaelstromEncoder};
use fec_simulator::fec::tart::{
    AdaptiveFecScheduler, TartDecoder, TartEncoder, TartFecScheduler, WindowStepScheduler,
};
use fec_simulator::fec::FecDecoder;
use fec_simulator::fec::FecEncoder;
use fec_simulator::node::decoder::{Decoder, DecoderFeedback};
use fec_simulator::node::dropper::Dropper;
use fec_simulator::node::encoder::Encoder;
use fec_simulator::Simulator;

#[derive(Clone, Debug)]
enum DropS {
    None,
    Uniform,
    Constant,
    GilbertEliot,
    Specific,
}

impl From<&str> for DropS {
    fn from(value: &str) -> Self {
        match value {
            "uniform" => Self::Uniform,
            "constant" => Self::Constant,
            "ge" => Self::GilbertEliot,
            "specific" => Self::Specific,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Debug)]
struct MaelstromLayering {
    layers: Vec<u64>,
}

impl From<String> for MaelstromLayering {
    fn from(value: String) -> Self {
        let tab = value.split(",");
        Self {
            layers: tab.map(|item| item.parse().unwrap()).collect(),
        }
    }
}

#[derive(Clone)]
enum Fec {
    None,
    Tart,
    Maelstrom,
}

impl From<&str> for Fec {
    fn from(value: &str) -> Self {
        match value {
            "tart" => Self::Tart,
            "maelstrom" => Self::Maelstrom,
            _ => Self::None,
        }
    }
}

#[derive(Parser)]
struct Args {
    /// Number of packets to run in a single simulation.
    #[clap(short = 'n')]
    nb_packets: u64,

    /// Uniform loss ratio [0, 1]. Also the 'p' value of the Gilbert-Elliot drop model.
    #[clap(long = "u-loss", default_value = "0.0")]
    u_loss_ratio: f64,

    ///The 'r' value of the Gilbert-Elliot drop model.
    #[clap(short = 'r', default_value = "1.0")]
    r_ge: f64,

    /// Step for the constant drop scheduler. Only used with a constant drop scheduler.
    #[clap(long = "constant-drop-step", default_value = "100")]
    constant_loss_step: u64,

    /// Sets the initial loss estimation to the drop rate.
    #[clap(long = "set-initial-loss")]
    set_initial_loss: bool,

    /// Sets the $\beta$ adaptive FEC parameter.
    #[clap(long = "beta", default_value = "1.0")]
    beta_fec: f64,

    /// Sets the $\alpha$ adaptive FEC parameter.
    #[clap(long = "alpha", default_value = "0.9")]
    alpha_fec: f64,

    /// Drop scheduler to use.
    #[clap(long = "drop", default_value = "none")]
    drop_scheduler: DropS,

    /// Number of source symbols between two feedbacks, if the FEC mechanism uses them.
    #[clap(long = "feedback", default_value = "500")]
    feedback_freq: u64,

    /// Max FEC window.
    #[clap(long = "window", default_value = "100")]
    fec_window: u64,

    /// Drop seed.
    #[clap(short = 's', default_value = "1")]
    drop_seed: u64,

    /// FEC mechanism to use.
    #[clap(short = 'f', long = "fec", default_value = "tart")]
    fec: Fec,

    /// Output directory.
    #[clap(short = 'd', long = "dir", default_value = ".")]
    directory: String,

    /// Use window scheduler instead of adaptive for TART.
    #[clap(short = 'w')]
    tart_window: bool,

    /// Activate dropper trace and store it in the path pointed to by the argument.
    #[clap(long = "dtrace")]
    drop_trace: Option<String>,

    /// Activate decoder trace and store it in the path pointed to by the argument.
    #[clap(long = "rtrace")]
    rec_trace: Option<String>,

    /// Maelstrom layering.
    #[clap(long = "layering", default_value = "1,20,40", value_parser = clap::value_parser!(MaelstromLayering))]
    maelstrom_layering: MaelstromLayering,
}

fn main() {
    env_logger::init();

    let args = Args::parse();
    let mut simulator = Simulator::new();

    // Add dropper.
    let drop_scheduler: Box<dyn DropScheduler> = match args.drop_scheduler {
        DropS::None => Box::new(NoDropScheduler {}),
        DropS::Constant => Box::new(ConstantDropScheduler::new(args.constant_loss_step)),
        DropS::Uniform => Box::new(UniformDropScheduler::new(args.u_loss_ratio, args.drop_seed)),
        DropS::GilbertEliot => Box::new(GilbertEliotDropSheduler::new_simple(
            args.u_loss_ratio,
            args.r_ge,
            args.drop_seed,
        )),
        DropS::Specific => {
            let mut scheduler = SpecificDropScheduler::new(100);
            scheduler.add_to_drop(&[20, 21]);
            Box::new(scheduler)
        }
    };
    info!("Chosen drop scheduler: {:?}", drop_scheduler);
    let mut dropper = Dropper::new(drop_scheduler);
    if args.drop_trace.is_some() {
        dropper.activate_trace();
    }
    simulator.set_dropper(dropper);

    let (encoder, mut decoder) = match args.fec {
        Fec::Maelstrom => get_maelstrom(&args),
        Fec::Tart => get_tart(&args),
        _ => (Encoder::new_simple(), Decoder::new_simple()),
    };
    simulator.set_encoder(encoder);
    if args.rec_trace.is_some() {
        decoder.activate_trace();
    }
    simulator.set_decoder(decoder);

    simulator.run(args.nb_packets).unwrap();

    println!(
        "Nb recovered: {}",
        simulator.get_decoder().get_nb_recovered()
    );
    println!(
        "And number missing: {}",
        simulator.get_sink().get_lost(args.nb_packets).len()
    );
    println!(
        "Number of erased packets ssy: {}",
        simulator.get_dropper().get_nb_ss_dropped()
    );
    println!(
        "And number of erased packets: {}",
        simulator.get_dropper().get_nb_dropped()
    );
    println!(
        "Ratio of dropped a posteriori: {} (received {})",
        simulator.get_dropper().get_dropped_ratio_posteriori(),
        simulator.get_dropper().get_nb_recv()
    );
    println!(
        "Number of sent repair: {} (for {} ss)",
        simulator.get_encoder().get_nb_rs(),
        simulator.get_encoder().get_nb_ss()
    );
    println!(
        "Number of duplicate packets: {} ({:?})",
        simulator.get_sink().get_duplicates().len(),
        simulator.get_sink().get_duplicates(),
    );

    to_csv(&simulator, &args).unwrap();

    if let Some(filepath) = args.drop_trace {
        let path = std::path::Path::new(&filepath);
        let mut wrt = csv::WriterBuilder::new()
            .has_headers(true)
            .from_path(path)
            .unwrap();

        wrt.write_record(["id", "is_repair", "is_dropped"])
            .unwrap();
        for &(id, is_repair, is_dropped) in simulator.get_dropper().get_trace().unwrap() {
            wrt.write_record(&[
                format!("{}", id),
                format!("{}", if is_repair { 1 } else { 0 }),
                format!("{}", if is_dropped { 1 } else { 0 }),
            ])
            .unwrap();
        }
    }
}

fn get_tart(args: &Args) -> (Encoder, Decoder) {
    let scheduler: Box<dyn TartFecScheduler> = if args.tart_window {
        Box::new(WindowStepScheduler::new(args.fec_window, 10))
    } else {
        let mut scheduler = AdaptiveFecScheduler::new(args.alpha_fec, args.fec_window);
        if args.set_initial_loss {
            scheduler.set_initial_loss_estimation(args.u_loss_ratio.max(1.0 / args.fec_window as f64));
        }
        scheduler.set_beta_fec(args.beta_fec);
        scheduler.set_alpha_fec(args.alpha_fec);
        Box::new(scheduler)
    };
    let tart_encoder = TartEncoder::new(scheduler, args.fec_window);
    let encoder = Encoder::new(FecEncoder::Tart(tart_encoder));

    let fec_decoder = FecDecoder::Tart(TartDecoder::new(args.fec_window));
    let feedback = DecoderFeedback::new(args.feedback_freq);
    let decoder = Decoder::new(fec_decoder, Some(feedback));

    (encoder, decoder)
}

fn get_maelstrom(args: &Args) -> (Encoder, Decoder) {
    let encoder = MaelstromEncoder::new(args.fec_window as usize, &args.maelstrom_layering.layers);
    let encoder = Encoder::new(FecEncoder::Maelstrom(encoder));

    let decoder = MaelstromDecoder::new(args.fec_window as usize * 20);
    let decoder = Decoder::new(FecDecoder::Maelstrom(decoder), None);

    (encoder, decoder)
}

fn to_csv(simulator: &Simulator, args: &Args) -> std::io::Result<()> {
    fs::create_dir_all(&args.directory)?;

    let pathname = format!(
        "{:?}-{:?}-{}-{}-{}.csv",
        simulator.get_encoder().get_fec_encoder(),
        args.drop_scheduler,
        args.u_loss_ratio,
        args.nb_packets,
        args.drop_seed
    );
    println!("Pathname: {:?}", &pathname);
    let path = std::path::Path::new(&args.directory).join(pathname);

    let mut wrt = csv::WriterBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    wrt.write_record([
        "n-repair",
        "n-lost",
        "n-recovered",
        "n-ss-drop",
        "n-drop",
        "ratio,post",
    ])?;
    wrt.write_record(&[
        format!("{}", simulator.get_encoder().get_nb_rs()),
        format!("{}", simulator.get_sink().get_lost(args.nb_packets).len()),
        format!(
            "{}",
            simulator
                .get_decoder()
                .get_nb_recovered()
                .saturating_sub(simulator.get_sink().get_duplicates().len() as u64)
        ),
        format!("{}", simulator.get_dropper().get_nb_ss_dropped()),
        format!("{}", simulator.get_dropper().get_nb_dropped()),
        format!("{}", simulator.get_dropper().get_dropped_ratio_posteriori()),
    ])?;

    if let Some(directory) = args.rec_trace.as_ref() {
        fs::create_dir_all(directory)?;

        let pathname = format!(
            "{:?}-{:?}-{}-{}-{}.csv",
            simulator.get_encoder().get_fec_encoder(),
            args.drop_scheduler,
            args.u_loss_ratio,
            args.nb_packets,
            args.drop_seed
        );
        let path = std::path::Path::new(directory).join(pathname);
        let mut wrt = csv::WriterBuilder::new()
            .has_headers(true)
            .from_path(path)
            .unwrap();

        wrt.write_record(["id", "delay"]).unwrap();
        for (id, delay) in simulator.get_sink().get_recovering_delay() {
            wrt.write_record(&[format!("{}", id), format!("{}", delay)])
                .unwrap();
        }
    }

    Ok(())
}
