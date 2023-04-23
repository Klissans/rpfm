//---------------------------------------------------------------------------//
// Copyright (c) 2017-2023 Ismael Gutiérrez González. All rights reserved.
//
// This file is part of the Rusted PackFile Manager (RPFM) project,
// which can be found here: https://github.com/Frodo45127/rpfm.
//
// This file is licensed under the MIT license, which can be found here:
// https://github.com/Frodo45127/rpfm/blob/master/LICENSE.
//---------------------------------------------------------------------------//

/*!
This module contains the code to load the icons used in the `TreeView`.
!*/

use qt_gui::QIcon;
use qt_gui::QStandardItem;

use qt_core::QString;

use cpp_core::CppBox;
use cpp_core::Ref;

use std::sync::atomic::AtomicPtr;

use rpfm_lib::files::{animpack, anim_fragment, anims_table, audio, esf, FileType, image, loc, matched_combat, pack, portrait_settings, rigidmodel, soundbank, text, text::*, unit_variant, video, uic};
use rpfm_lib::{REGEX_DB, REGEX_PORTRAIT_SETTINGS};
use rpfm_ui_common::ASSETS_PATH;

use crate::pack_tree::{ROOT_NODE_TYPE_EDITABLE_PACKFILE, ROOT_NODE_TYPE};
use crate::utils::{atomic_from_cpp_box, ref_from_atomic_ref};

//-------------------------------------------------------------------------------//
//                              Enums & Structs
//-------------------------------------------------------------------------------//

/// This enum contains the variants used to decide which icon corresponds to which item in the `TreeView`,
pub enum IconType {

    // For normal PackFiles. `true` if it's editable, `false` if it's read-only.
    Pack(bool),

    // For files. Includes the path without the PackFile's name on it.
    File(String),
}

/// This struct is used to hold all the QIcons used by the `TreeView`s.
pub struct Icons {
    pub packfile_editable: AtomicPtr<QIcon>,
    pub packfile_locked: AtomicPtr<QIcon>,
    pub folder: AtomicPtr<QIcon>,
    pub file: AtomicPtr<QIcon>,

    pub animpack: AtomicPtr<QIcon>,
    pub anim_fragment: AtomicPtr<QIcon>,
    pub anims_table: AtomicPtr<QIcon>,
    pub audio: AtomicPtr<QIcon>,
    pub db: AtomicPtr<QIcon>,
    pub esf: AtomicPtr<QIcon>,

    pub image_generic: AtomicPtr<QIcon>,
    pub image_png: AtomicPtr<QIcon>,
    pub image_jpg: AtomicPtr<QIcon>,
    pub image_tga: AtomicPtr<QIcon>,
    pub image_gif: AtomicPtr<QIcon>,

    pub loc: AtomicPtr<QIcon>,
    pub matched_combat: AtomicPtr<QIcon>,
    pub portrait_settings: AtomicPtr<QIcon>,
    pub sound_bank: AtomicPtr<QIcon>,

    pub text_generic: AtomicPtr<QIcon>,
    pub text_csv: AtomicPtr<QIcon>,
    pub text_cpp: AtomicPtr<QIcon>,
    pub text_md: AtomicPtr<QIcon>,
    pub text_json: AtomicPtr<QIcon>,
    pub text_html: AtomicPtr<QIcon>,
    pub text_txt: AtomicPtr<QIcon>,
    pub text_xml: AtomicPtr<QIcon>,
    pub text_lua: AtomicPtr<QIcon>,
    pub text_js: AtomicPtr<QIcon>,
    pub text_css: AtomicPtr<QIcon>,

    pub rigid_model: AtomicPtr<QIcon>,
    pub unit_variant: AtomicPtr<QIcon>,
    pub uic: AtomicPtr<QIcon>,
    pub video: AtomicPtr<QIcon>,
}

//-------------------------------------------------------------------------------//
//                              Implementations
//-------------------------------------------------------------------------------//

impl Icons {
    pub unsafe fn new() -> Self {
        Self {
            packfile_editable: atomic_from_cpp_box(Self::load_icon("packfile_editable", "application-x-compress")),
            packfile_locked: atomic_from_cpp_box(Self::load_icon("packfile_locked", "application-x-xz-compressed-tar")),
            folder: atomic_from_cpp_box(Self::load_icon("folder", "folder-orange")),
            file: atomic_from_cpp_box(Self::load_icon("file", "none")),
            animpack: atomic_from_cpp_box(Self::load_icon("animpack", "package-x-generic")),
            anim_fragment: atomic_from_cpp_box(Self::load_icon("anim_fragment", "animation-stage")),
            anims_table: atomic_from_cpp_box(Self::load_icon("anims_table", "gnumeric-pivottable")),
            audio: atomic_from_cpp_box(Self::load_icon("audio", "audio-mp3")),
            db: atomic_from_cpp_box(Self::load_icon("db", "application-sql")),
            esf: atomic_from_cpp_box(Self::load_icon("esf", "application-x-bzdvi")),
            image_generic: atomic_from_cpp_box(Self::load_icon("image_generic", "image-x-generic")),
            image_png: atomic_from_cpp_box(Self::load_icon("image_png", "image-png")),
            image_jpg: atomic_from_cpp_box(Self::load_icon("image_jpg", "image-jpeg")),
            image_tga: atomic_from_cpp_box(Self::load_icon("image_tga", "image-tga")),
            image_gif: atomic_from_cpp_box(Self::load_icon("image_gif", "image-gif")),
            loc: atomic_from_cpp_box(Self::load_icon("loc", "text-x-gettext-translation")),
            matched_combat: atomic_from_cpp_box(Self::load_icon("matched_combat", "view-table-of-contents-ltr")),
            portrait_settings: atomic_from_cpp_box(Self::load_icon("portrait_settings", "x-office-contact")),
            sound_bank: atomic_from_cpp_box(Self::load_icon("sound_bank", "view-bank")),
            text_generic: atomic_from_cpp_box(Self::load_icon("text_generic", "text-x-generic")),
            text_csv: atomic_from_cpp_box(Self::load_icon("text_csv", "text-csv")),
            text_cpp: atomic_from_cpp_box(Self::load_icon("text_cpp", "text-x-c++src")),
            text_md: atomic_from_cpp_box(Self::load_icon("text_md", "text-markdown")),
            text_json: atomic_from_cpp_box(Self::load_icon("text_json", "application-json")),
            text_html: atomic_from_cpp_box(Self::load_icon("text_html", "text-html")),
            text_txt: atomic_from_cpp_box(Self::load_icon("text_txt", "text-plain")),
            text_xml: atomic_from_cpp_box(Self::load_icon("text_xml", "text-xml")),
            text_lua: atomic_from_cpp_box(Self::load_icon("text_lua", "text-x-lua")),
            text_js: atomic_from_cpp_box(Self::load_icon("text_js", "text-javascript")),
            text_css: atomic_from_cpp_box(Self::load_icon("text_css", "text-css")),
            rigid_model: atomic_from_cpp_box(Self::load_icon("rigid_model", "application-x-blender")),
            unit_variant: atomic_from_cpp_box(Self::load_icon("unit_variant", "application-vnd.openxmlformats-officedocument.spreadsheetml.sheet")),
            uic: atomic_from_cpp_box(Self::load_icon("uic", "application-x-designer")),
            video: atomic_from_cpp_box(Self::load_icon("video", "video-webm")),
        }
    }

    pub unsafe fn load_icon(icon_name: &str, icon_name_fallback: &str) -> CppBox<QIcon> {
        let mut icon = QIcon::from_q_string(&QString::from_std_str(format!("{}/icons/{}.png", ASSETS_PATH.to_string_lossy(), icon_name)));

        if icon.is_null() || icon.available_sizes_0a().count_0a() == 0 {
            icon = QIcon::from_theme_1a(&QString::from_std_str(icon_name));

            if icon.is_null() || icon.available_sizes_0a().count_0a() == 0 {
                icon = QIcon::from_theme_1a(&QString::from_std_str(icon_name_fallback));
            }
        }

        icon
    }

    /// This function is used to get the icon corresponding to an IconType.
    pub fn icon(&self, icon_type: IconType) -> Ref<QIcon> {
        ref_from_atomic_ref(match icon_type {

            // For PackFiles.
            IconType::Pack(editable) => {
                if editable { &self.packfile_editable }
                else { &self.packfile_locked }
            },

            // For files, logic based on lib's file type guesser.
            IconType::File(path) => {

                // First, try with extensions.
                let path = path.to_lowercase();

                // TODO: Add autodetection to these, somehow
                //--Anim,
                //--GroupFormations,
                //--UIC,

                if path.ends_with(pack::EXTENSION) {
                    &self.packfile_editable
                }

                else if path.ends_with(loc::EXTENSION) {
                    &self.loc
                }

                else if path.ends_with(rigidmodel::EXTENSION) {
                    &self.rigid_model
                }

                else if path.ends_with(animpack::EXTENSION) {
                    &self.animpack
                }

                else if path.ends_with(video::EXTENSION) {
                    &self.video
                }

                else if audio::EXTENSIONS.iter().any(|x| path.ends_with(x)) {
                    &self.audio
                }

                else if cfg!(feature = "support_soundbank") && path.ends_with(soundbank::EXTENSION) {
                    &self.sound_bank
                }

                else if image::EXTENSIONS.iter().any(|x| path.ends_with(x)) {
                    if path.ends_with(".jpg") { &self.image_jpg }
                    else if path.ends_with(".jpeg") { &self.image_jpg }
                    else if path.ends_with(".dds") { &self.image_generic }
                    else if path.ends_with(".tga") { &self.image_tga }
                    else if path.ends_with(".png") { &self.image_png }
                    else if path.ends_with(".gif") { &self.image_gif }
                    else { &self.image_generic }
                }

                else if cfg!(feature = "support_uic") && path.starts_with(uic::BASE_PATH) && uic::EXTENSIONS.iter().any(|x| path.ends_with(x) || !path.contains('.')) {
                    &self.uic
                }

                else if let Some((_, text_type)) = text::EXTENSIONS.iter().find(|(extension, _)| path.ends_with(extension)) {
                    match text_type {
                        TextFormat::Html => &self.text_html,
                        TextFormat::Xml => &self.text_xml,
                        TextFormat::Lua => &self.text_lua,
                        TextFormat::Cpp => &self.text_cpp,
                        TextFormat::Plain => &self.text_txt,
                        TextFormat::Markdown => &self.text_md,
                        TextFormat::Json => &self.text_json,
                        TextFormat::Css => &self.text_css,
                        TextFormat::Js => &self.text_js,
                    }
                }

                else if path.ends_with(unit_variant::EXTENSION) {
                    &self.unit_variant
                }

                else if esf::EXTENSIONS.iter().any(|x| path.ends_with(*x)) {
                    &self.esf
                }

                // If that failed, try types that need to be in a specific path.
                else if matched_combat::BASE_PATHS.iter().any(|x| path.starts_with(*x)) && path.ends_with(matched_combat::EXTENSION) {
                    &self.matched_combat
                }

                else if path.starts_with(anims_table::BASE_PATH) && path.ends_with(anims_table::EXTENSION) {
                    &self.anims_table
                }

                else if path.starts_with(anim_fragment::BASE_PATH) && anim_fragment::EXTENSIONS.iter().any(|x| path.ends_with(*x)) {
                    &self.anim_fragment
                }

                // If that failed, check if it's in a folder which is known to only have specific files.
                // Microoptimization: check the path before using the regex. Regex is very, VERY slow.
                else if path.starts_with("db/") && REGEX_DB.is_match(&path) {
                    &self.db
                }

                else if path.ends_with(portrait_settings::EXTENSION) && REGEX_PORTRAIT_SETTINGS.is_match(&path) {
                    &self.portrait_settings
                }

                // If we reach this... we're clueless. Leave it unknown.
                else {
                    &self.file
                }
            }
        })
    }

    pub unsafe fn set_standard_item_icon(&self, item: &QStandardItem, file_type: Option<&FileType>) {
         let icon = ref_from_atomic_ref(
            match file_type {
                Some(file_type) => match file_type {
                    FileType::Pack => {
                        if item.data_1a(ROOT_NODE_TYPE).to_int_0a() == ROOT_NODE_TYPE_EDITABLE_PACKFILE {
                            &self.packfile_editable
                        } else {
                            &self.packfile_locked
                        }
                    },
                    FileType::Anim => &self.file,
                    FileType::AnimFragment => &self.anim_fragment,
                    FileType::AnimPack => &self.animpack,
                    FileType::AnimsTable => &self.anims_table,
                    FileType::Audio => &self.audio,
                    FileType::DB => &self.db,
                    FileType::ESF => &self.esf,
                    FileType::GroupFormations => &self.file,
                    FileType::Image => {
                        let name = item.text().to_std_string();
                        if name.ends_with(".jpg") { &self.image_jpg }
                        else if name.ends_with(".jpeg") { &self.image_jpg }
                        else if name.ends_with(".dds") { &self.image_generic }
                        else if name.ends_with(".tga") { &self.image_tga }
                        else if name.ends_with(".png") { &self.image_png }
                        else if name.ends_with(".gif") { &self.image_gif }
                        else { &self.image_generic }
                    }
                    FileType::Loc => &self.loc,
                    FileType::MatchedCombat => &self.matched_combat,
                    FileType::PortraitSettings => &self.portrait_settings,
                    FileType::RigidModel => &self.rigid_model,
                    FileType::SoundBank => &self.sound_bank,
                    FileType::Text => {
                        let name = item.text().to_std_string();
                        match text::EXTENSIONS.iter().find(|(extension, _)| name.ends_with(extension)) {
                            Some((_, text_type)) => {
                                match text_type {
                                    TextFormat::Html => &self.text_html,
                                    TextFormat::Xml => &self.text_xml,
                                    TextFormat::Lua => &self.text_lua,
                                    TextFormat::Cpp => &self.text_cpp,
                                    TextFormat::Plain => &self.text_txt,
                                    TextFormat::Markdown => &self.text_md,
                                    TextFormat::Json => &self.text_json,
                                    TextFormat::Css => &self.text_css,
                                    TextFormat::Js => &self.text_js,
                                }
                            },
                            None => &self.text_generic,
                        }
                    },
                    FileType::UIC => &self.uic,
                    FileType::UnitVariant => &self.unit_variant,
                    FileType::Video => &self.video,
                    FileType::Unknown => &self.file,

                },
                None => &self.folder,
            }
        );
        item.set_icon(icon);
    }
}
