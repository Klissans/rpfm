//---------------------------------------------------------------------------//
// Copyright (c) 2017-2024 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

/*!
Module with the slots for PackFile Settings Views.
!*/

use qt_core::QBox;
use qt_core::SlotNoArgs;

use std::rc::Rc;
use std::sync::Arc;

use rpfm_ui_common::clone;

use crate::app_ui::AppUI;
use crate::packedfile_views::{ViewType, View};
use crate::packfile_contents_ui::PackFileContentsUI;
use crate::UI_STATE;
use super::PackFileSettingsView;

//-------------------------------------------------------------------------------//
//                              Enums & Structs
//-------------------------------------------------------------------------------//

/// This struct contains the slots of the view of a PackFile Settings.
pub struct PackFileSettingsSlots {
    pub apply: QBox<SlotNoArgs>,
}

//-------------------------------------------------------------------------------//
//                             Implementations
//-------------------------------------------------------------------------------//

/// Implementation for `PackFileSettingsSlots`.
impl PackFileSettingsSlots {

    /// This function creates the entire slot pack for PackFile Settings Views.
    pub unsafe fn new(
        view: &Arc<PackFileSettingsView>,
        app_ui: &Rc<AppUI>,
        pack_file_contents_ui: &Rc<PackFileContentsUI>,
    )  -> Self {

        // Slot to apply settings changes.
        let apply = SlotNoArgs::new(view.get_ref_apply_button(), clone!(
            app_ui,
            pack_file_contents_ui=> move || {
                for view in &*UI_STATE.get_open_packedfiles() {
                    if let ViewType::Internal(View::PackSettings(_)) = view.view_type() {
                        let _ = view.save(&app_ui, &pack_file_contents_ui);
                        break;
                    }
                }
            }
        ));

        // Return the slots, so we can keep them alive for the duration of the view.
        Self {
            apply,
        }
    }
}
