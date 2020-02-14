use clap::Clap;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use ez_io::{ReadE, WriteE, MagicNumberCheck};
use std::net::TcpStream;
use std::io::{Write, BufWriter};
use std::fs::File;
use std::mem::transmute;

static PROTOCOL_VERSION: u32 = 1;
static MAGIC_NUMBER: &str = "NETDUMP";

// ---------------------------- CLI Argument Parsing stuff ----------------------------

/// Client for netdump running on a Wii
#[derive(Clap)]
#[clap(author = "Marime Gui <lepro.guillaume@gmail.com>")]
struct Opts {
    /// Hostname of the Wii to connect to
    #[clap(short = "a", long = "address", value_name = "HOSTNAME")]
    host_address: String,
    /// Hostname of the Wii to connect to
    #[clap(short = "p", long = "port", value_name = "PORT", default_value = "25565")]
    port: u16,
    /// Choose what to get from the disc
    #[clap(subcommand)]
    process: Process,
}

#[derive(Clap)]
enum Process {
    /// Dumps the Game, BCA and Info to three separate files
    #[clap(name = "full")]
    Full(FullDump),
    /// Dumps game ISO only
    #[clap(name = "game")]
    Game(GameDump),
    /// Dumps game BCA only, will fail if dumping GC game
    #[clap(name = "bca")]
    BCA(BCADump),
    /// Returns Disc type (GC, Wii Single-sided or Wii Double-sided), Game ID and Game Name
    #[clap(name = "info")]
    Info(InfoRead),
}

#[derive(Clap)]
struct FullDump {
    /// Where the files will be written to
    #[clap(short = "o", long = "output", value_name = "DIRECTORY", default_value = ".")]
    location: String,
}

#[derive(Clap)]
struct GameDump {
    /// Where to write game dump
    #[clap(short = "o", long = "output", value_name = "FILE", default_value = "./game.iso")]
    filepath: String,
    /// Output to stdout rather than to a file. If this is set, 'output' option will be ignored
    #[clap(short = "s", long = "stdout")]
    stdout: bool,
}

#[derive(Clap)]
struct BCADump {
    /// Where to write BCA dump
    #[clap(short = "o", long = "output", value_name = "FILE", default_value = "./game.bca")]
    filepath: String,
    /// Output to stdout rather than to a file. If this is set, 'output' option will be ignored
    #[clap(short = "s", long = "stdout")]
    stdout: bool,
}

#[derive(Clap)]
struct InfoRead {
    /// Write info dump as JSON to a file
    #[clap(short = "o", long = "output", value_name = "FILE")]
    filepath: Option<String>,
}

// ---------------------------- Network Protocol Stuff ----------------------------

#[derive(Copy, Clone)]
#[repr(u32)]
enum Commands {
    /// Ask to disconnect nicely
    Disconnect = 0xFFFFFFFF,
    /// Shutdown console, acts like we're disconnecting
    Shutdown = 0xFFFFFFFE,
    /// Ejects the Disc
    EjectDisc = 1,
    /// Get info about the disc
    GetDiscInfo = 2,
    /// Dumps BCA
    DumpBCA = 3,
    /// Dumps main Game
    DumpGame = 4,
}

#[derive(Copy, Clone, FromPrimitive)]
#[repr(u32)]
enum CommandAnswers {
    ProtocolError = 0xFFFFFFFF,
    NoDisc = 0xFFFFFFFE,
    OK = 0,
    DiscInfo = 1,
    BCA = 2,
    Game = 3,
}

// ---------------------------- Main Code ----------------------------

fn main() {
    let opts: Opts = Opts::parse();
    
    let mut stream = TcpStream::connect(format!("{}:{}", opts.host_address, opts.port)).expect("Failed to connect to the Wii"); // BufWriter maybe ?
    stream.write_all(&MAGIC_NUMBER.as_bytes()).unwrap(); // Magic Number
    stream.write_be_to_u32(PROTOCOL_VERSION).unwrap(); // Protocol Version

    match opts.process {
        Process::BCA(bca) => {
            if bca.stdout {
                unimplemented!();
            } else {
                stream.write_be_to_u32(Commands::DumpBCA as u32).unwrap(); // Command, 'as' is meh

                let file_writer = &mut BufWriter::new(File::create(bca.filepath).expect("Failed to open file"));

                stream.check_magic_number(&MAGIC_NUMBER.as_bytes()).unwrap(); // Check Magic Number
                stream.check_magic_number(unsafe {&transmute::<u32, [u8; 4]>(PROTOCOL_VERSION.to_be())}).unwrap(); // Check Protocol Version, Meh transmute

                match CommandAnswers::from_u32(stream.read_be_to_u32().unwrap()) {
                    Some(CommandAnswers::BCA) => unimplemented!(),
                    Some(CommandAnswers::ProtocolError) => eprintln!("Unknown Protocol-related error, can't proceed"),
                    Some(CommandAnswers::NoDisc) => eprintln!("No Disc in Drive, can't proceed"),
                    _ => eprintln!("Weird response from Wii, disconnecting"),
                }
            }
        }
        _ => unimplemented!(),
    }

}
