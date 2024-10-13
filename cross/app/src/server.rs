use crc::{Crc, CRC_32_ISCSI};
use dimmer_communication::{ChannelDuty, ChannelEnabled, CHANNELS_COUNT, ClientCommand, ClientCommandResult, ClientCommandResultType, ClientCommandType, GroupFrequency, PINS_COUNT, PinState, Request, Response};
use postcard::{from_bytes_crc32, to_slice_crc32};
use crate::communication::{BufferWriter, LedState};
use crate::errors::Error;
use crate::pwm_service::PwmChannels;
use crate::storage::Storage;

const MAX_PROCESSED_IDS: usize = 50;


pub struct Server {
    processed_ids: [u32; MAX_PROCESSED_IDS],
    processed_count: u8,
    processed_overflow: bool,
    //buff: [u8; 256],
    crc: Crc::<u32>,
    device_id: [u32; 3], 
    device_no: Option<u8>, 
}

impl Server {
    pub fn new(device_id: [u32; 3], device_no: Option<u8>) -> Self {
        Server {
            processed_ids: [0; MAX_PROCESSED_IDS],
            processed_count: 0,
            processed_overflow: false,
            //buff: [0u8; 256],
            crc: Crc::<u32>::new(&CRC_32_ISCSI),
            device_id, 
            device_no,
        }
    }

    pub fn idle(
        &mut self, 
        input_data: &[u8],
        out_buffer: &mut dyn BufferWriter, 
        storage: &mut Storage, 
        channels: &mut PwmChannels,
        leds_state: &mut LedState,
    ) -> Result<(), Error> {


        if input_data.len() == 1 {
            let value = input_data[0];
            if value < 8 {
                let on = (value & 0x04) >> 2;
                let led = value & 0x03;
                leds_state.set_high(led, on == 1);
            } else if value > 15 && value < 32 {
                leds_state.set_mask(value & 0x0F);
            }
        }

        // out_buffer.add_str("value: ")?;
        // out_buffer.add(input_data)?;

        let request_parsed: Result<Request, Error> = from_bytes_crc32(input_data, self.crc.digest())
            .map_err(|err| { Error::SerializedDataError(err) });
        // to_slice_crc32(&ClientCommandType::SetChannelDuty(ChannelDuty::new(1, 0.7)), &mut buff, crc.digest()).map(|b| {
        //     out_buffer.add(b)
        // }).map_err(|_| {Error::SerializedDataError})?;
        
        match request_parsed { 
            Ok(request) => {
                match request { 
                    Request::BroadcastRequest(command) => {
                        Ok(())
                    }
                    Request::DeviceByIdRequest(command) => {
                        Ok(())
                    }
                    Request::DeviceByNoRequest(command) => {
                        if self.device_no.is_some_and(|no| command.is_no(no)) {
                            self.process_client_command(command.command, input_data, out_buffer, storage, channels, leds_state)
                        } else { 
                            Ok(())
                        }
                    }
                }
            },
            Err(e) => {
                let response_parsed: Result<Response, Error> = from_bytes_crc32(input_data, self.crc.digest())
                    .map_err(|err| { Error::SerializedDataError(err) });
                match response_parsed {
                    Ok(response) => {
                        //response from other device to master - ignore
                        Ok(())
                    }, 
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(0, e.to_command_error()), out_buffer)
                    }
                }
            }
        }
    }


    fn process_client_command(&mut self,
                              command: ClientCommand,
                              input_data: &[u8],
                              out_buffer: &mut dyn BufferWriter,
                              storage: &mut Storage,
                              channels: &mut PwmChannels,
                              leds_state: &mut LedState,
    ) -> Result<(), Error> {

        match command.data {
            ClientCommandType::SetChannelEnabled ( ChannelEnabled { channel, enabled } ) => {
                match channels.set_enabled_channel(channel, enabled) {
                    Ok(_) => {
                        send_answer(&ClientCommandResult::channel_enabled(command.id,channel, enabled), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllChannelsSameEnabled { enabled } => {
                channels.set_enabled_all_same(enabled);
                send_answer(&ClientCommandResult::all_channels_same_enabled(command.id, enabled), out_buffer)
            },
            ClientCommandType::SetAllChannelsEnabled (enabled) => {
                channels.set_enabled_all(enabled);
                send_answer(&ClientCommandResult::all_channels_enabled(command.id, enabled), out_buffer)
            },
            ClientCommandType::SetChannelDuty(data) => {
                match channels.set_channel_duty(data.channel, data.duty) {
                    Ok(duty) => {
                        send_answer(&ClientCommandResult::channel_duty(command.id,data.channel, duty), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllChannelsSameDuty(duty) => {
                channels.set_all_duties_same(duty);
                send_answer(&ClientCommandResult::all_channels_same_duty(command.id, duty), out_buffer)
            },
            ClientCommandType::SetAllChannelsDuty(duties) => {
                channels.set_all_duties(duties);
                send_answer(&ClientCommandResult::all_channels_duty(command.id, duties), out_buffer)
            },
            ClientCommandType::GetChannelDuty { channel } => {
                match channels.get_channel_duty(channel) {
                    Ok(duty) => {
                        send_answer(&ClientCommandResult::channel_duty(command.id, channel, duty), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetAllChannelsDuty => {
                let mut data: [f32; CHANNELS_COUNT] = [0.0; CHANNELS_COUNT];
                for i in 0..CHANNELS_COUNT {
                    data[i] = channels.get_channel_duty(i as u8)?;
                }
                send_answer(&ClientCommandResult::all_channels_duty(command.id, data), out_buffer)
            },
            ClientCommandType::GetChannelCount => {
                let count = channels.get_channels_count();
                send_answer(&ClientCommandResult::channel_count(command.id, count), out_buffer)
            },
            ClientCommandType::SetGroupFrequency (data) => {
                match storage.read_pwm_settings() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_group_freq(data.group, data.frequency)?;
                        storage.save_pwm_settings(&pwm_settings)
                        //TODO: restart board
                        //no answer needed due to restart
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetGroupFrequency(group) => {
                match storage.read_pwm_settings() {
                    Ok(pwm_settings) => {
                        let freq = pwm_settings.get_group_freq(group)?;
                        send_answer(&ClientCommandResult::group_frequency(command.id, group, freq), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllGroupsSameFrequency (freq) => {
                match storage.read_pwm_settings() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_all_groups_same_freq(freq);
                        match storage.save_pwm_settings(&pwm_settings) {
                            Ok(_) => {
                                //TODO: restart board
                                //no answer needed due to restart
                                Ok(())
                            },
                            Err(e) => {
                                return send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                            }
                        }
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::SetAllGroupsFrequency (data) => {
                match storage.read_pwm_settings() {
                    Ok(mut pwm_settings) => {
                        pwm_settings.set_all_groups_freq(data);
                        match storage.save_pwm_settings(&pwm_settings) {
                            Ok(_) => {
                                //TODO: restart board
                                //no answer needed due to restart
                                Ok(())
                            }
                            Err(e) => {
                                return send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                            }
                        }
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetAllGroupsFrequency => {
                match storage.read_pwm_settings() {
                    Ok(pwm_settings) => {
                        let freq = pwm_settings.get_all_groups_freq();
                        send_answer(&ClientCommandResult::all_groups_frequency(command.id, freq), out_buffer)
                    },
                    Err(e) => {
                        send_answer(&ClientCommandResult::err(command.id, e.to_command_error()), out_buffer)
                    }
                }
            },
            ClientCommandType::GetDateTimestamp => {
                //TODO: implement
                Ok(())
            },
            ClientCommandType::SetDateTimestamp { timestamp: u64 } => {
                //TODO: implement
                Ok(())
            },
            ClientCommandType::SetPinState (PinState { pin, state }) => {
                leds_state.set_high(pin, state);
                let state = leds_state.is_high(pin);
                send_answer(&ClientCommandResult::pin_state(command.id, pin, state), out_buffer)
            },
            ClientCommandType::GetPinState (pin) => {
                let state = leds_state.is_high(pin);
                send_answer(&ClientCommandResult::pin_state(command.id, pin, state), out_buffer)
            },
            ClientCommandType::SetAllPinsState (states) => {
                for i in 0..PINS_COUNT {
                    leds_state.set_high(i as u8, states[i]);
                }
                send_answer(&ClientCommandResult::all_pins_state(command.id, states), out_buffer)
            },
            ClientCommandType::SetAllPinsSameState (state) => {
                for i in 0..PINS_COUNT {
                    leds_state.set_high(i as u8, state);
                }
                send_answer(&ClientCommandResult::all_pin_same_state(command.id, state), out_buffer)
            },
            ClientCommandType::GetAllPinsState => {
                let mut data: [bool; PINS_COUNT] = [false; PINS_COUNT];
                for i in 0..8 {
                    data[i] = leds_state.is_high(i as u8);
                }
                send_answer(&ClientCommandResult::all_pins_state(command.id, data), out_buffer)
            },
            ClientCommandType::GetDeviceId => {
                send_answer(&ClientCommandResult::device_id(command.id, self.device_id), out_buffer)
            },
        }
    }
}


fn send_answer(value: &ClientCommandResult, out_buffer: &mut dyn BufferWriter) -> Result<(), Error> {
    let mut buff = [0u8; 256];
    let crc = Crc::<u32>::new(&CRC_32_ISCSI);
    match to_slice_crc32(value, &mut buff, crc.digest()) {
        Ok(b) => {
            out_buffer.add(b)
        },
        Err(e) => {
            Err(Error::SerializedDataError(e))
        }
    }
}