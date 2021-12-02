#![forbid(unsafe_code)]
#![warn(
	// Turn on extra language lints.
	future_incompatible,
	missing_abi,
	nonstandard_style,
	rust_2018_idioms,
	// Disabled due to <https://github.com/rust-lang/rust/issues/69952>.
	// single_use_lifetimes,
	trivial_casts,
	trivial_numeric_casts,
	unused,
	unused_crate_dependencies,
	unused_import_braces,
	unused_lifetimes,
	unused_qualifications,

	// Turn on extra Rustdoc lints.
	rustdoc::all,

	// Turn on extra Clippy lints.
	clippy::cargo,
	clippy::pedantic,
)]

use clap::{App, AppSettings, Arg, ArgMatches};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

/// Reads a two-byte length-prefixed string from a file.
fn read_string<R: Read>(r: &mut R) -> std::io::Result<String> {
	let mut len = [0_u8; 2];
	r.read_exact(&mut len)?;
	let len = u16::from_be_bytes(len).into();
	let mut buffer = vec![0; len];
	r.read_exact(&mut buffer)?;
	String::from_utf8(buffer).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Writes a two-byte length-prefixed string to a file.
fn write_string<W: Write>(w: &mut W, s: &str) -> std::io::Result<()> {
	let s = s.as_bytes();
	w.write_all(
		&TryInto::<u16>::try_into(s.len())
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
			.to_be_bytes(),
	)?;
	w.write_all(s)
}

/// Immersive-specific sample data.
#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
struct ImmersiveSampleData {
	/// The name of the mineral.
	pub mineral: String,

	/// The name of the liquid deposit.
	pub liquid: String,

	/// The time when the sample was taken.
	pub timestamp: u64,
}

impl ImmersiveSampleData {
	/// Reads the Immersive-specific portion of a sample from a file.
	pub fn read_from<R: Read>(r: &mut R) -> std::io::Result<Self> {
		let mineral = read_string(r)?;
		let liquid = read_string(r)?;
		let mut timestamp = [0_u8; 8];
		r.read_exact(&mut timestamp)?;
		let timestamp = u64::from_be_bytes(timestamp);
		Ok(Self {
			mineral,
			liquid,
			timestamp,
		})
	}

	/// Writes the Immersive-specific portion of a sample to a file.
	pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
		write_string(w, &self.mineral)?;
		write_string(w, &self.liquid)?;
		w.write_all(&self.timestamp.to_be_bytes())
	}
}

impl Display for ImmersiveSampleData {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
		write!(
			f,
			"{}, {}, timestamp {}",
			self.mineral, self.liquid, self.timestamp
		)
	}
}

/// Per-mod data about a sample.
#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
enum SampleData {
	/// Data for an Immersive sample.
	Immersive(ImmersiveSampleData),

	/// The ore name for a TerraFirmaCraft sample.
	TerraFirmaCraft(String),

	/// The ore name for a Geolosys sample.
	Geolosys(String),
}

impl Display for SampleData {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
		match self {
			Self::Immersive(i) => write!(f, "Immersive {}", i),
			Self::TerraFirmaCraft(i) => write!(f, "TFC {}", i),
			Self::Geolosys(i) => write!(f, "Geolosys {}", i),
		}
	}
}

/// A single sample.
#[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
struct Sample {
	/// The dimension ID containing the deposit.
	pub dimension: i32,

	/// The chunk X coordinate.
	pub x: i32,

	/// The chunk Z coordinate.
	pub z: i32,

	/// The per-mod data about the sample.
	pub data: SampleData,
}

impl Sample {
	/// Reads a sample from a file.
	pub fn read_from<R: Read>(r: &mut R) -> std::io::Result<Self> {
		let mut source_mod = [0_u8; 4];
		r.read_exact(&mut source_mod)?;
		let source_mod = u32::from_be_bytes(source_mod);
		if source_mod == 0 || source_mod == 1 || source_mod == 2 {
			let mut dimension = [0_u8; 4];
			r.read_exact(&mut dimension)?;
			let dimension = i32::from_be_bytes(dimension);
			let mut buf4 = [0_u8; 4];
			r.read_exact(&mut buf4)?;
			let x = i32::from_be_bytes(buf4);
			r.read_exact(&mut buf4)?;
			let z = i32::from_be_bytes(buf4);
			let data = match source_mod {
				0 => SampleData::Immersive(ImmersiveSampleData::read_from(r)?),
				1 => SampleData::TerraFirmaCraft(read_string(r)?),
				2 => SampleData::Geolosys(read_string(r)?),
				_ => unreachable!(),
			};
			Ok(Self {
				dimension,
				x,
				z,
				data,
			})
		} else {
			Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"invalid sample type",
			))
		}
	}

	/// Reads a counted list of samples from a file.
	pub fn read_list_from<R: Read>(r: &mut R) -> std::io::Result<Vec<Self>> {
		let mut count = [0_u8; 4];
		r.read_exact(&mut count)?;
		let count = u32::from_be_bytes(count);
		let count: usize = count
			.try_into()
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
		let mut ret = Vec::with_capacity(count);
		for _ in 0..count {
			ret.push(Self::read_from(r)?);
		}
		Ok(ret)
	}

	/// Writes a sample to a file.
	pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
		w.write_all(
			&match self.data {
				SampleData::Immersive(_) => 0_u32,
				SampleData::TerraFirmaCraft(_) => 1_u32,
				SampleData::Geolosys(_) => 2_u32,
			}
			.to_be_bytes(),
		)?;
		w.write_all(&self.dimension.to_be_bytes())?;
		w.write_all(&self.x.to_be_bytes())?;
		w.write_all(&self.z.to_be_bytes())?;
		match self.data {
			SampleData::Immersive(ref data) => data.write_to(w),
			SampleData::TerraFirmaCraft(ref ore) | SampleData::Geolosys(ref ore) => {
				write_string(w, ore)
			}
		}
	}

	/// Writes a counted list of samples to a file.
	pub fn write_list_to<W: Write>(w: &mut W, data: &[Self]) -> std::io::Result<()> {
		let count: u32 = data
			.len()
			.try_into()
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
		w.write_all(&count.to_be_bytes())?;
		for i in data {
			i.write_to(w)?;
		}
		Ok(())
	}
}

impl Display for Sample {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
		write!(
			f,
			"Dimension {}, X={}, Z={}: {}",
			self.dimension, self.x, self.z, self.data
		)
	}
}

/// Reads a sample file.
fn read_file(file: impl AsRef<Path>) -> std::io::Result<Vec<Sample>> {
	Sample::read_list_from(&mut BufReader::new(File::open(file)?))
}

/// Writes a sample file.
fn write_file(file: impl AsRef<Path>, samples: &[Sample]) -> std::io::Result<()> {
	let mut writer = BufWriter::new(File::create(file)?);
	Sample::write_list_to(&mut writer, samples)?;
	let mut writer = writer.into_inner()?;
	writer.flush()?;
	writer.sync_all()?;
	Ok(())
}

/// Given a string, converts it to a desired type, and exits with a given message if it is not
/// convertible.
fn convert<T: std::str::FromStr>(s: impl AsRef<str>, message: &str) -> T {
	if let Ok(n) = s.as_ref().parse() {
		n
	} else {
		eprintln!("{}", message);
		std::process::exit(1);
	}
}

/// Implements the `edit` command.
fn do_edit(matches: &ArgMatches<'_>, file: impl AsRef<Path>) -> std::io::Result<()> {
	let mut samples = read_file(&file)?;
	let dimension: i32 = convert(
		matches.value_of("dimension").unwrap(),
		"Dimension ID must be an integer",
	);
	let x: i32 = convert(
		matches.value_of("x").unwrap(),
		"X coordinate must be an integer",
	);
	let z: i32 = convert(
		matches.value_of("z").unwrap(),
		"Z coordinate must be an integer",
	);
	let mineral = matches.value_of("mineral");
	let liquid = matches.value_of("liquid");
	let ore = matches.value_of("ore");
	let mut found = false;
	for i in &mut samples {
		if i.dimension == dimension && i.x == x && i.z == z {
			match i.data {
				SampleData::Immersive(ref mut data) => {
					if let Some(m) = mineral {
						data.mineral = m.to_owned();
						found = true;
					}
					if let Some(l) = liquid {
						data.liquid = l.to_owned();
						found = true;
					}
				}
				SampleData::TerraFirmaCraft(ref mut data) | SampleData::Geolosys(ref mut data) => {
					if let Some(o) = ore {
						*data = o.to_owned();
						found = true;
					}
				}
			}
		}
	}
	if found {
		write_file(file, &samples)?;
	} else if ore.is_some() {
		eprintln!(
			"No TFC or Geolosys sample found in dimension {} at X={}, Z={}",
			dimension, x, z
		);
		std::process::exit(1);
	} else {
		eprintln!(
			"No Immersive sample found in dimension {} at X={}, Z={}",
			dimension, x, z
		);
		std::process::exit(1);
	}
	Ok(())
}

fn main() -> std::io::Result<()> {
	let matches = App::new("barotool")
		.author(clap::crate_authors!())
		.about("Manipulates Minecraft Mineral Tracker data files.")
		.version(clap::crate_version!())
		.setting(AppSettings::InferSubcommands)
		.setting(AppSettings::SubcommandRequiredElseHelp)
		.setting(AppSettings::VersionlessSubcommands)
		.arg(
			Arg::with_name("file")
				.help("The .samples2 file to operate on")
				.required(true),
		)
		.subcommand(
			App::new("edit")
				.about("Modifies one sample in a file.")
				.arg(
					Arg::with_name("dimension")
						.help("The dimension ID containing the sample")
						.required(true)
						.allow_hyphen_values(true),
				)
				.arg(
					Arg::with_name("x")
						.help("The X coordinate of the sample to modify")
						.required(true)
						.allow_hyphen_values(true),
				)
				.arg(
					Arg::with_name("z")
						.help("The Z coordinate of the sample to modify")
						.required(true)
						.allow_hyphen_values(true),
				)
				.arg(
					Arg::with_name("mineral")
						.long("mineral")
						.short("m")
						.help("The Immersive mineral deposit text to change the sample to")
						.takes_value(true),
				)
				.arg(
					Arg::with_name("liquid")
						.long("liquid")
						.short("l")
						.help("The Immersive liquid reservoir text to change the sample to")
						.takes_value(true),
				)
				.arg(
					Arg::with_name("ore")
						.long("ore")
						.short("o")
						.help("The TerraFirmaCraft or Geolosys ore name to change the sample to")
						.takes_value(true)
						.conflicts_with("mineral")
						.conflicts_with("liquid")
						.required_unless_one(&["mineral", "liquid"]),
				),
		)
		.subcommand(App::new("list").about("Lists the samples in a file."))
		.get_matches();
	let file = matches.value_of_os("file").unwrap();
	if let Some(matches) = matches.subcommand_matches("edit") {
		do_edit(matches, file)
	} else if matches.subcommand_matches("list").is_some() {
		for sample in read_file(file)? {
			println!("{}", sample);
		}
		Ok(())
	} else {
		panic!("no subcommand")
	}
}
