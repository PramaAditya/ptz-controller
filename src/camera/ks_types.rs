use windows::core::GUID;

/// CLSID_SystemDeviceEnum: `{62BE5D10-60EB-11d0-BD3B-00A0C911CE86}`
pub const CLSID_SYSTEM_DEVICE_ENUM: GUID =
    GUID::from_u128(0x62be5d10_60eb_11d0_bd3b_00a0c911ce86);

/// CLSID_VideoInputDeviceCategory: `{860BB310-5D01-11d0-BD3B-00A0C911CE86}`
pub const CLSID_VIDEO_INPUT_DEVICE_CATEGORY: GUID =
    GUID::from_u128(0x860bb310_5d01_11d0_bd3b_00a0c911ce86);

/// PROPSETID_VIDCAP_CAMERACONTROL: `{C6E13370-30AC-11D0-A18C-00A0C9118956}`
pub const PROPSETID_VIDCAP_CAMERACONTROL: GUID =
    GUID::from_u128(0xc6e13370_30ac_11d0_a18c_00a0c9118956);

pub const KSPROPERTY_CAMERACONTROL_PAN_RELATIVE: u32 = 10;
pub const KSPROPERTY_CAMERACONTROL_TILT_RELATIVE: u32 = 11;
pub const KSPROPERTY_CAMERACONTROL_ZOOM_RELATIVE: u32 = 13;

pub const KSPROPERTY_TYPE_SET: u32 = 0x0000_0002;

pub const KSPROPERTY_CAMERACONTROL_FLAGS_RELATIVE: u32 = 0x0000_0010;

/// Exact 24-byte `KSIDENTIFIER` / `KSPROPERTY` header with 8-byte alignment on x64.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
pub struct KsProperty {
    pub set: GUID,
    pub id: u32,
    pub flags: u32,
}

/// Exact 40-byte `KSPROPERTY_CAMERACONTROL_S` layout required by `IKsPropertySet` on x64.
/// Passing 36 bytes causes `ERROR_INSUFFICIENT_BUFFER` (`0x8007007A`).
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
pub struct KsPropertyCameraControlS {
    pub property: KsProperty,
    pub value: i32,
    pub flags: u32,
    pub capabilities: u32,
    pub _padding: u32,
}

const _: () = {
    assert!(std::mem::size_of::<KsProperty>() == 24);
    assert!(std::mem::align_of::<KsProperty>() == 8);
    assert!(std::mem::size_of::<KsPropertyCameraControlS>() == 40);
    assert!(std::mem::align_of::<KsPropertyCameraControlS>() == 8);
};

impl KsPropertyCameraControlS {
    pub fn new_relative_set(prop_id: u32, value: i32) -> Self {
        Self {
            property: KsProperty {
                set: PROPSETID_VIDCAP_CAMERACONTROL,
                id: prop_id,
                flags: KSPROPERTY_TYPE_SET,
            },
            value,
            flags: KSPROPERTY_CAMERACONTROL_FLAGS_RELATIVE,
            capabilities: 0,
            _padding: 0,
        }
    }
}
