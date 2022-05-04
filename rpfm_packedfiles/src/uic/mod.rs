//---------------------------------------------------------------------------//
// Copyright (c) 2017-2022 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

/*!
Module with all the code to interact with UI Components.

UI Components are binary files that form the ui of TW Games.
They have no extension (mostly), and I heard they're a pain in the ass to work with.
!*/

use anyhow::Result;

use rpfm_common::{decoder::Decoder, encoder::Encoder, schema::Schema};
use crate::{Decodeable, Encodeable, PackedFileType};

const SIGNATURE: &str = "Version";
const VERSION_SIZE: usize = 3;

/// Size of the header of an UIC PackedFile.
pub const HEADER_SIZE: usize = 10;

pub const EXTENSION: &str = ".cml";

//---------------------------------------------------------------------------//
//                              Enum & Structs
//---------------------------------------------------------------------------//

/// This holds an entire UI Component decoded in memory.
#[derive(PartialEq, Clone, Debug, Default)]
pub struct UIC {
    version: u32,
}

//---------------------------------------------------------------------------//
//                           Implementation of Text
//---------------------------------------------------------------------------//

/// Implementation of `UIC`.
impl UIC {

    pub fn is_ui_component(data: &[u8]) -> bool {
        match data.decode_string_u8(0, 7) {
            Ok(signature) => signature == SIGNATURE,
            Err(_) => false,
        }
    }

    /// This function tries to read the header of an UIC PackedFile from raw data.
    fn read_header(packed_file_data: &[u8]) -> Result<u32> {
        let _signature = packed_file_data.decode_string_u8(0, SIGNATURE.len())?;
        let version = packed_file_data.decode_string_u8(SIGNATURE.len(), VERSION_SIZE)?.parse::<u32>()?;

        Ok(version)
    }
}


impl Decodeable for UIC {

    fn file_type(&self) -> PackedFileType {
        PackedFileType::UIC
    }

    fn decode(packed_file_data: &[u8], extra_data: Option<(&Schema, &str, bool)>) -> Result<Self> {
        let version = Self::read_header(packed_file_data)?;

        // If we've reached this, we've successfully decoded the entire UI.
        Ok(Self {
            version,
        })
    }
}

impl Encodeable for UIC {
    fn encode(&self) -> Vec<u8> {
        let mut data = vec![];
        data.encode_string_u8(SIGNATURE);
        data.encode_integer_u32(self.version);

        data
    }
}
