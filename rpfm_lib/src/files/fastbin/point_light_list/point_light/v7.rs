//---------------------------------------------------------------------------//
// Copyright (c) 2017-2023 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

use crate::binary::ReadBytes;
use crate::error::Result;

use super::*;

//---------------------------------------------------------------------------//
//                           Implementation of PointLight
//---------------------------------------------------------------------------//

impl PointLight {

    pub(crate) fn read_v7<R: ReadBytes>(&mut self, data: &mut R, extra_data: &Option<DecodeableExtraData>) -> Result<()> {
        self.position = Position {
            x: data.read_f32()?,
            y: data.read_f32()?,
            z: data.read_f32()?,
        };

        self.radius = data.read_f32()?;

        self.colour = Colour {
            r: data.read_f32()?,
            g: data.read_f32()?,
            b: data.read_f32()?,
        };
        self.colour_scale = data.read_f32()?;

        // TODO: place some more lights and check this, because on the test files is all 0 and has a 4 in a boolean.
        self.animation_type = data.read_u8()?;
        self.colour_min = data.read_f32()?;
        self.random_offset = data.read_f32()?;

        self.params = Params {
            x: data.read_f32()?,
            y: data.read_f32()?,
        };

        self.falloff_type = data.read_sized_string_u8()?;

        // TODO: How the fuck do we get a 4 here?!!! It's supposed to be a boolean.
        self.lf_relative = data.read_u8()?;
        self.height_mode = data.read_sized_string_u8()?;
        self.light_probes_only = data.read_bool()?;
        self.pdlc_mask = data.read_u64()?;
        self.flags = Flags::decode(data, extra_data)?;

        Ok(())
    }

    pub(crate) fn write_v7<W: WriteBytes>(&mut self, buffer: &mut W, extra_data: &Option<EncodeableExtraData>) -> Result<()> {
        buffer.write_f32(self.position.x)?;
        buffer.write_f32(self.position.y)?;
        buffer.write_f32(self.position.z)?;

        buffer.write_f32(self.radius)?;

        buffer.write_f32(self.colour.r)?;
        buffer.write_f32(self.colour.g)?;
        buffer.write_f32(self.colour.b)?;

        buffer.write_f32(self.colour_scale)?;

        buffer.write_u8(self.animation_type)?;
        buffer.write_f32(self.colour_min)?;
        buffer.write_f32(self.random_offset)?;

        buffer.write_f32(self.params.x)?;
        buffer.write_f32(self.params.y)?;

        buffer.write_sized_string_u8(&self.falloff_type)?;
        buffer.write_u8(self.lf_relative)?;
        buffer.write_sized_string_u8(&self.height_mode)?;
        buffer.write_bool(self.light_probes_only)?;
        buffer.write_u64(self.pdlc_mask)?;

        self.flags.encode(buffer, extra_data)?;

        Ok(())
    }
}

