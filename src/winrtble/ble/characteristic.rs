// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

use super::super::bindings;
use crate::{api::WriteType, Error, Result};

use bindings::windows::devices::bluetooth::generic_attribute_profile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
    GattCommunicationStatus, GattValueChangedEventArgs, GattWriteOption,
};
use bindings::windows::foundation::{EventRegistrationToken, TypedEventHandler};
use bindings::windows::storage::streams::{DataReader, DataWriter};
use log::{debug, trace};

pub type NotifiyEventHandler = Box<dyn Fn(Vec<u8>) + Send>;

impl Into<GattWriteOption> for WriteType {
    fn into(self) -> GattWriteOption {
        match self {
            WriteType::WithoutResponse => GattWriteOption::WriteWithoutResponse,
            WriteType::WithResponse => GattWriteOption::WriteWithResponse,
        }
    }
}

pub struct BLECharacteristic {
    characteristic: GattCharacteristic,
    notify_token: Option<EventRegistrationToken>,
}

unsafe impl Send for BLECharacteristic {}
unsafe impl Sync for BLECharacteristic {}

impl BLECharacteristic {
    pub fn new(characteristic: GattCharacteristic) -> Self {
        BLECharacteristic {
            characteristic,
            notify_token: None,
        }
    }

    pub fn write_value(&self, data: &[u8], write_type: WriteType) -> Result<()> {
        let writer = DataWriter::new().unwrap();
        writer.write_bytes(data)?;
        let buffer = writer.detach_buffer()?;
        let result = self
            .characteristic
            .write_value_with_option_async(&buffer, write_type.into())?
            .get()?;
        if GattCommunicationStatus::Success == result {
            Ok(())
        } else {
            Err(Error::Other(format!("Windows UWP threw error on write: {:?}", result)))
        }
    }

    pub fn read_value(&self) -> Result<Vec<u8>> {
        let result = self
            .characteristic
            .read_value_async()?
            .get()?;
        if result.status()? == GattCommunicationStatus::Success {
            let value = result.value()?;
            let reader = DataReader::from_buffer(&value)?;
            let len = reader.unconsumed_buffer_length()? as usize;
            let mut input = vec![0u8; len];
            reader.read_bytes(&mut input[0..len])?;
            Ok(input)
        } else {
            Err(Error::Other(format!("Windows UWP threw error on read: {:?}", result)))
        }
    }

    pub fn subscribe(&mut self, on_value_changed: NotifiyEventHandler) -> Result<()> {
        let value_handler = TypedEventHandler::new(
            move |_: &Option<GattCharacteristic>, args: &Option<GattValueChangedEventArgs>| {
                if let Some(args) = args {
                    let value = args.characteristic_value()?;
                    let reader = DataReader::from_buffer(&value)?;
                    let len = reader.unconsumed_buffer_length()? as usize;
                    let mut input: Vec<u8> = vec![0u8; len];
                    reader.read_bytes(&mut input[0..len])?;
                    trace!("changed {:?}", input);
                    on_value_changed(input);
                }
                Ok(())
            },
        );
        let token = self.characteristic.value_changed(&value_handler)?;
        self.notify_token = Some(token);
        let config = GattClientCharacteristicConfigurationDescriptorValue::Notify;
        let status = self
            .characteristic
            .write_client_characteristic_configuration_descriptor_async(config)?
            .get()?;
        trace!("subscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(format!("Windows UWP threw error on subscribe: {:?}", status)))
        }
    }

    pub fn unsubscribe(&mut self) -> Result<()> {
        if let Some(token) = &self.notify_token {
            self.characteristic.remove_value_changed(token)?;
        }
        self.notify_token = None;
        let config = GattClientCharacteristicConfigurationDescriptorValue::None;
        let status = self
            .characteristic
            .write_client_characteristic_configuration_descriptor_async(config)?
            .get()?;
        trace!("unsubscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(format!("Windows UWP threw error on unsubscribe: {:?}", status)))
        }
    }
}

impl Drop for BLECharacteristic {
    fn drop(&mut self) {
        if let Some(token) = &self.notify_token {
            let result = self.characteristic.remove_value_changed(token);
            if let Err(err) = result {
                debug!("Drop:remove_connection_status_changed {:?}", err);
            }
        }
    }
}
