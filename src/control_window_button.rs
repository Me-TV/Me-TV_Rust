/*
 *  Me TV — It's TV for me computer.
 *
 *  A GTK+/GStreamer client for watching and recording DVB.
 *
 *  Copyright © 2017–2020  Russel Winder
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program. If not, see <http://www.gnu.org/licenses/>.
 */

use std::cell::RefCell;
use std::rc::Rc;

use gtk;
use gtk::prelude::*;

use crate::channels_data::{encode_to_mrl, get_channel_name_of_logical_channel_number};
use crate::control_window::ControlWindow;
use crate::dialogs::display_an_error_dialog;
use crate::frontend_manager::FrontendId;
use crate::frontend_window::FrontendWindow;
use crate::input_event_codes;
use crate::metvcombobox::{MeTVComboBox, MeTVComboBoxExt};
use crate::preferences;
use crate::remote_control::TargettedKeystroke;

/// A `ControlWindowButton` is a `gtk::Box` but there is no inheritance so use
/// composition.
#[derive(Clone, Debug)]
pub struct ControlWindowButton {
    pub control_window: Rc<ControlWindow>, // FrontendWindow instance needs access to this.
    pub frontend_id: FrontendId, // ControlWindow instance needs access to this for searching.
    pub widget: gtk::Box, // ControlWindow instance needs access to this for packing.
    pub frontend_button: gtk::ToggleButton, // FrontendWindow needs access to this.
    pub channel_selector: MeTVComboBox, // FrontendWindow needs read access to this.
    frontend_window: RefCell<Option<Rc<FrontendWindow>>>,
    channel_number_dialog: gtk::Dialog,
    channel_number_entry: gtk::Entry,
}

impl ControlWindowButton {
    /// Construct a new button representing an available front end.
    ///
    /// The adapter and frontend numbers for the label for a toggle button that is used
    /// to start and stop a frontend window displaying the stream for that frontend. Below
    /// is a drop down list button to select the channel to tune the front end to.
    ///
    /// This function is executed in the GTK event loop thread.
    pub fn new(control_window: &Rc<ControlWindow>, fei: &FrontendId) -> Rc<ControlWindowButton> {
        let frontend_id = fei.clone();
        let frontend_button = gtk::ToggleButton::with_label(
            format!("adaptor{}\nfrontend{}", frontend_id.adapter, frontend_id.frontend).as_ref()
        );
        let channel_selector = MeTVComboBox::new_with_model(&control_window.channels_data_sorter);
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.pack_start(&frontend_button, true, true, 0);
        widget.pack_start(&channel_selector, true, true, 0);
        let channel_number_dialog = gtk::Dialog::new();
        let channel_number_entry = gtk::Entry::new();
        let max_length = 3;
        channel_number_entry.set_max_length(max_length);
        channel_number_entry.set_max_width_chars(max_length);
        channel_number_entry.set_alignment(1.0);
        channel_number_entry.set_progress_fraction(1.0);
        channel_number_entry.show();
        channel_number_dialog.set_title("Channel Number");
        channel_number_dialog.get_content_area().pack_start(&channel_number_entry, false, false, 10);
        let control_window_button = Rc::new(ControlWindowButton {
            control_window: control_window.clone(),
            frontend_id,
            widget,
            frontend_button,
            channel_selector,
            frontend_window: RefCell::new(None),
            channel_number_dialog,
            channel_number_entry,
        });
        control_window_button.reset_active_channel();
        control_window_button.channel_selector.connect_changed({
            let c_w_b = control_window_button.clone();
            move |_| Self::on_channel_changed(&c_w_b, c_w_b.channel_selector.get_active().unwrap())
        });
        control_window_button.frontend_button.connect_toggled({
            let c_w_b = control_window_button.clone();
            move |_| {
                if c_w_b.control_window.is_channels_store_loaded() {
                    Self::toggle_button(&c_w_b);
                } else {
                    display_an_error_dialog(Some(&c_w_b.control_window.window), "No channel file, so no channel list, so cannot play a channel.");
                }
            }
        });
        control_window_button
    }

    /// Set the active channel to index 0.
    pub fn reset_active_channel(&self) {  // Used in control_window.rs
        self.channel_selector.set_active(Some(0));
        if let Some(ref frontend_window) = *self.frontend_window.borrow() {
            frontend_window.channel_selector.set_active(Some(0));
            frontend_window.fullscreen_channel_selector.set_active(Some(0));
        }
    }

    /// Set the state of all the channel control widgets.
    fn set_channel_index(&self, channel_index: u32) {
        let current = self.channel_selector.get_active().unwrap();
        if current != channel_index {
            self.channel_selector.set_active(Some(channel_index));
        }
        if let Some(ref frontend_window) = *self.frontend_window.borrow() {
            let fe_current = frontend_window.channel_selector.get_active().unwrap();
            if fe_current != channel_index {
                frontend_window.channel_selector.set_active(Some(channel_index));
            }
            let fs_fe_current = frontend_window.fullscreen_channel_selector.get_active().unwrap();
            if fs_fe_current != channel_index {
                frontend_window.fullscreen_channel_selector.set_active(Some(channel_index));
            }
        }
    }

    /// Toggle the button.
    ///
    /// This function is called after the change of state of the frontend_button.
    fn toggle_button(control_window_button: &Rc<ControlWindowButton>) { // Used in control_window.rs
        if control_window_button.frontend_button.get_active() {
            if control_window_button.control_window.is_channels_store_loaded() {
                let frontend_window = match FrontendWindow::new(control_window_button.clone()) {
                    Ok(frontend_window) => frontend_window,
                    Err(_) => {
                        display_an_error_dialog(Some(&control_window_button.control_window.window), "Could not create a frontend window, most likely because\na GStreamer engine could not be created.");
                        return;
                    },
                };
                match control_window_button.frontend_window.replace(Some(frontend_window)) {
                    Some(_) => panic!("Inconsistent state of frontend,"),
                    None => {},
                };
            }
            // TODO Should there be an else activity here?
        } else {
            match control_window_button.frontend_window.replace(None) {
                Some(ref frontend_window) => frontend_window.stop(),
                None => panic!("Inconsistent state of frontend,"),
            }
        }
    }

    /// Callback for an observed channel change.
    pub fn on_channel_changed(control_window_button: &Rc<ControlWindowButton>, channel_index: u32) { // Used in frontend_window.rs
        // TODO status is Option<u32> apparently which isn't a great bool value.
        let status = control_window_button.frontend_button.get_active();
        if let Some(ref frontend_window) = *control_window_button.frontend_window.borrow() {
            if status {
                // Do not stop the frontend completely just change what is being displayed on it.
                frontend_window.engine.stop();
                let window_title = "Me TV – ".to_string() + &control_window_button.channel_selector.get_active_text().unwrap();
                let f_w = &frontend_window.window;
                f_w.set_title(&window_title);
                let h_b = f_w.get_titlebar().unwrap().downcast::<gtk::HeaderBar>().unwrap();
                h_b.set_title(Some(&window_title));
                // TODO Need to clear the area in the gtk::DrawingArea or a gtk::GLArea
                //   to avoid keeping the last video frame when it is a switch to radio.
                //   See https://github.com/Me-TV/Me-TV/issues/29
                let w = frontend_window.engine.video_widget.clone();
                match w.clone().downcast::<gtk::DrawingArea>() {
                    Ok(_d) => {
                        // TODO Clear the background area.
                    },
                    Err(_) => {
                        match w.clone().downcast::<gtk::GLArea>() {
                            Ok(g) => {
                                let _c = g.get_context().unwrap();
                                // TODO Clear the background area.
                            },
                            Err(e) => panic!("Widget is neither gtk::DrawingArea or gtk::GLArea: {}", e),
                        }
                    },
                }
                println!("========  Channel changed callback called");
                // TODO Why does changing channel on the FrontendWindow result in three calls here.
            }
            control_window_button.set_channel_index(channel_index);
            let channel_name = control_window_button.channel_selector.get_active_text().unwrap();
            frontend_window.engine.set_mrl(&encode_to_mrl(&channel_name));
            preferences::set_last_channel(channel_name, true);
            if status {
                // TODO Must handle not being able to tune to a channel better than panicking.
                frontend_window.engine.play();
            }
        }
    }

    /// Process a targetted keystroke.
    pub fn process_targetted_keystroke(&self, tk: &TargettedKeystroke) {
        assert_eq!(self.frontend_id, tk.frontend_id);
        match tk.keystroke {
            input_event_codes::KEY_CHANNELUP => {
                if tk.value > 0 {
                    let selector = &self.channel_selector;
                    let index = selector.get_active().unwrap();
                    // TODO Need to stop going beyond the number of channels there are.
                    selector.set_active(Some(index + 1));
                }
            }
            input_event_codes::KEY_CHANNELDOWN => {
                if tk.value > 0 {
                    let selector = &self.channel_selector;
                    let index = selector.get_active().unwrap();
                    if index > 0 {
                        selector.set_active(Some(index - 1));
                    }
                }
            }
            input_event_codes::KEY_VOLUMEUP => {
                if tk.value > 0 {
                    if let Some(ref f_w) = *self.frontend_window.borrow() {
                        let button = &f_w.volume_button;
                        let volume = button.get_value();
                        let adjustment = button.get_adjustment();
                        let increment = adjustment.get_step_increment();
                        let maximum = adjustment.get_upper();
                        let new_volume = volume + increment;
                        if new_volume < maximum {
                            button.set_value(new_volume);
                        } else {
                            button.set_value(maximum);
                        }
                    }
                }
            },
            input_event_codes::KEY_VOLUMEDOWN => {
                if tk.value > 0 {
                    if let Some(ref f_w) = *self.frontend_window.borrow() {
                        let button = &f_w.volume_button;
                        let volume = button.get_value();
                        let adjustment = button.get_adjustment();
                        let increment = adjustment.get_step_increment();
                        let minimum = adjustment.get_lower();
                        let new_volume = volume - increment;
                        if new_volume > minimum {
                            button.set_value(new_volume);
                        } else {
                            button.set_value(minimum);
                        }
                    }
                }
            },
            // These seem to be the keystrokes returned by the digit buttons on a remote control.
            input_event_codes::KEY_NUMERIC_0 ..= input_event_codes::KEY_NUMERIC_9 => {
                if tk.value == 1 {
                    self.process_numeric_keystroke(tk);
                }
            },
            x => println!("Got an unprocessed keystroke {}", x),
        }
    }

    /// Change the channel to the one collected by the `Entry`.
    fn change_channel_after_keystrokes(&self, channel_number: &str) {
        let channel_number = channel_number.parse::<u16>().unwrap();
        match get_channel_name_of_logical_channel_number(channel_number) {
            Some(channel_name) => {
                let index = {
                    let model = &self.control_window.channels_data_sorter;
                    let iterator = model.get_iter_first().unwrap();
                    let mut index = 0u32;
                    let mut success = false;
                    loop {
                        let name = model.get_value(&iterator, 1).get::<String>().unwrap().unwrap();
                        if name == channel_name {
                            success = true;
                            break
                        }
                        if ! model.iter_next(&iterator) { break }
                        index += 1;
                    }
                    if ! success { panic!("Failed to find {} in the data model", channel_name); }
                    index
                };
                self.set_channel_index(index);
            },
            None => println!("Failed to find channel name from channel number."),
        }
        self.channel_number_entry.set_text("");
        self.channel_number_entry.set_progress_fraction(1.0);
        self.channel_number_dialog.hide();
    }

    /// Process a numeric keystroke, most likely from a remote.
    ///
    /// Displays a dialogue which displays the digits received so far. If there are
    /// three digits present or there has been a delay of 3 seconds since the last digit
    /// then the input is assumed to be the channel number the user wants to switch to.
    fn process_numeric_keystroke(&self, tk: &TargettedKeystroke) {
        let digit = match tk.keystroke {
            input_event_codes::KEY_NUMERIC_0 => 0,
            input_event_codes::KEY_NUMERIC_1 => 1,
            input_event_codes::KEY_NUMERIC_2 => 2,
            input_event_codes::KEY_NUMERIC_3 => 3,
            input_event_codes::KEY_NUMERIC_4 => 4,
            input_event_codes::KEY_NUMERIC_5 => 5,
            input_event_codes::KEY_NUMERIC_6 => 6,
            input_event_codes::KEY_NUMERIC_7 => 7,
            input_event_codes::KEY_NUMERIC_8 => 8,
            input_event_codes::KEY_NUMERIC_9 => 9,
            x => panic!("Got a keystroke that it is impossible to get at this point: {}", x),
        };
        let dialog = &self.channel_number_dialog;
        dialog.show_all();
        let entry = &self.channel_number_entry;
        let max_length = entry.get_max_length() as usize;
        let fraction = |s: &str| -> f64 { 1.0 - (s.len() as f64) / (max_length as f64) };
        let text = entry.get_text().to_string() + &digit.to_string();
        entry.set_text(&text);
        entry.set_progress_fraction(fraction(&text));
        if text.len() >= max_length {
            self.change_channel_after_keystrokes(&text)
        } else {
            glib::timeout_add_seconds_local(3, {
                let s = self.clone();
                let e = entry.clone();
                let t = text.clone();
                move || {
                    if e.get_text() == t {
                        s.change_channel_after_keystrokes(&t);
                    }
                    Continue(false)
                }
            });
        }
    }
}
