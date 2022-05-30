//---------------------------------------------------------------------------//
// Copyright (c) 2017-2022 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

//! This module contains the Pack functions that are specific to PFH0 Packs.
//!
//! All the functions here are internal, so they should be either private or
//! public only within this crate.

use std::io::{BufReader, Cursor, prelude::*};

use crate::binary::{ReadBytes, WriteBytes};
use crate::error::{RLibError, Result};
use crate::files::{pack::*, RFile};

impl Pack {

    /// This function reads a `Pack` of version 0 from raw data, returning the index where it finished reading.
    pub(crate) fn read_pfh0<R: ReadBytes>(&mut self, data: &mut R) -> Result<u64> {
        let data_len = data.len()?;

        // Read the info about the indexes to use it later.
        let packs_count = data.read_u32()?;
        let packs_index_size = data.read_u32()?;
        let files_count = data.read_u32()?;
        let files_index_size = data.read_u32()?;

        // Optimization: we only really need the header of the Pack, not the data, and reads, if performed from disk, are expensive.
        // So we get all the data from the header to the end of the indexes to memory and put it in a buffer, so we can read it faster.
        let buffer_data = data.read_slice((packs_index_size as u64 + files_index_size as u64) as usize, true)?;
        let mut buffer_mem = BufReader::new(Cursor::new(buffer_data));

        // Check that the position of the data we want to get is actually valid.
        let mut data_pos = data.stream_position()? + buffer_mem.stream_position()? + packs_index_size as u64 + files_index_size as u64;
        if data_len < data_pos {
            return Err(RLibError::PackFileIndexesNotComplete)
        }

        // Get the Packs this Pack depends on, if any.
        for _ in 0..packs_count {
            self.dependencies.push(buffer_mem.read_string_u8_0terminated()?);
        }

        // Get the Files in the Pack.
        for _ in 0..files_count {
            let size = buffer_mem.read_u32()?;
            let path = buffer_mem.read_string_u8_0terminated()?;

            // Build the File as a LazyLoaded file by default.
            let file = RFile::new_from_container(self, size, false, None, data_pos, 0, &path);
            self.add_file(file)?;

            data_pos += u64::from(size);
        }

        // Return our current position on the data section for further checks.
        Ok(data_pos)
    }

    /// This function writes a `Pack` of version 0 into the provided buffer.
    pub(crate) fn write_pfh0<W: WriteBytes>(&mut self, buffer: &mut W, _test_mode: bool) -> Result<()> {

        // We need our files sorted before trying to write them. But we don't want to duplicate
        // them on memory. And we also need to load them to memory on the pack. So...  we do this.
        let mut sorted_files = self.files.iter_mut().collect::<Vec<(&String, &mut RFile)>>();
        sorted_files.sort_unstable_by_key(|(path, _)| path.to_lowercase());

        // Optimization: we process the sorted files in parallel, so we can speedup loading/compression.
        // Sadly, this requires us to make a double iterator to actually catch the errors.
        let (files_index, files_data): (Vec<_>, Vec<_>) = sorted_files.par_iter_mut()
            .map(|(path, file)| {

                // This unwrap is actually safe.
                let data = file.encode(true, true)?.unwrap();

                // 5 because 4 (size) + 1 (null).
                let file_index_entry_len = 5 + path.len();

                let mut file_index_entry = Vec::with_capacity(file_index_entry_len);
                file_index_entry.write_u32(data.len() as u32)?;
                file_index_entry.write_string_u8_0terminated(path)?;
                Ok((file_index_entry, data))
            }).collect::<Result<Vec<(Vec<u8>, Vec<u8>)>>>()?
            .into_par_iter()
            .unzip();

        let files_index = files_index.into_par_iter().flatten().collect::<Vec<_>>();
        let files_data = files_data.into_par_iter().flatten().collect::<Vec<_>>();

        // Build the dependencies index on memory. This one is never big, so no need of par_iter.
        let mut dependencies_index = vec![];
        for dependency in &self.dependencies {
            dependencies_index.write_string_u8_0terminated(dependency)?;
        }

        // Write the entire header to a memory buffer.
        let mut header = vec![];
        header.write_string_u8(self.header.pfh_version.value())?;
        header.write_u32(self.header.bitmask.bits | self.header.pfh_file_type.value())?;
        header.write_u32(self.dependencies.len() as u32)?;
        header.write_u32(dependencies_index.len() as u32)?;
        header.write_u32(sorted_files.len() as u32)?;
        header.write_u32(files_index.len() as u32)?;

        // Finally, write everything in one go.
        buffer.write_all(&header)?;
        buffer.write_all(&dependencies_index)?;
        buffer.write_all(&files_index)?;
        buffer.write_all(&files_data)?;

        Ok(())
    }
}
