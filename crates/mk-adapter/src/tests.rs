use std::str::FromStr;

use mk_core::ControlCode;

use crate::{
    compile_policy, AdapterDefinition, AdapterTransport, CompilePolicyError, DescriptorFingerprint,
    DeviceDescriptor, DriverOperation, NormalizedCapabilities, OneToOneMapping, PolicyLimits,
    SelectionError, SupportRegistry, APPLE_VENDOR_ID, MAX_DRIVER_POLICY_OPERATIONS,
};

// Synthetic, sanitized HID descriptor for three keyboard usages. The product
// IDs below are fixture-reserved values and do not represent Apple hardware.
const SYNTHETIC_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0x04, 0x29, 0x06, 0x15, 0x00, 0x25,
    0x01, 0x75, 0x01, 0x95, 0x03, 0x81, 0x02, 0x75, 0x05, 0x95, 0x01, 0x81, 0x03, 0xc0,
];
const FIXTURE_PRODUCT_ID: u16 = 0xff01;

fn source_capabilities() -> NormalizedCapabilities {
    NormalizedCapabilities::new(
        [
            ControlCode::keyboard(0x04),
            ControlCode::keyboard(0x05),
            ControlCode::keyboard(0x06),
        ],
        [],
        1,
        0,
    )
    .unwrap()
}

fn destination_capabilities() -> NormalizedCapabilities {
    NormalizedCapabilities::new(
        [],
        [
            ControlCode::keyboard(0x14),
            ControlCode::keyboard(0x1a),
            ControlCode::keyboard(0x08),
        ],
        0,
        8,
    )
    .unwrap()
}

fn fixture_definition(
    model: &str,
    transport: AdapterTransport,
) -> AdapterDefinition {
    AdapterDefinition::new(
        model,
        FIXTURE_PRODUCT_ID,
        transport,
        DescriptorFingerprint::of(SYNTHETIC_DESCRIPTOR),
        source_capabilities(),
    )
}

fn fixture_descriptor(
    transport: AdapterTransport,
    report_descriptor: &[u8],
) -> DeviceDescriptor<'_> {
    DeviceDescriptor {
        vendor_id: APPLE_VENDOR_ID,
        product_id: FIXTURE_PRODUCT_ID,
        transport,
        report_descriptor,
    }
}

#[test]
fn descriptor_fingerprint_has_a_golden_sha256_value() {
    let expected = DescriptorFingerprint::from_str(
        "3e7257f280f4a678b31b64817bb3f9f247b5844051db0f7a5a5c4ba35063f031",
    )
    .unwrap();

    assert_eq!(DescriptorFingerprint::of(SYNTHETIC_DESCRIPTOR), expected);
    assert_eq!(expected.to_string(), expected.to_string().to_lowercase());
}

#[test]
fn exact_identity_transport_and_fingerprint_match() {
    let registry = SupportRegistry::from_qualified_definitions([
        fixture_definition("synthetic-usb-fixture", AdapterTransport::Usb),
    ]);

    let selected = registry
        .select(&fixture_descriptor(
            AdapterTransport::Usb,
            SYNTHETIC_DESCRIPTOR,
        ))
        .unwrap();

    assert_eq!(selected.model(), "synthetic-usb-fixture");
    assert!(selected
        .capabilities()
        .supports_input(ControlCode::keyboard(0x04)));
}

#[test]
fn changed_descriptor_fingerprint_is_rejected() {
    let registry = SupportRegistry::from_qualified_definitions([
        fixture_definition("synthetic-usb-fixture", AdapterTransport::Usb),
    ]);
    let mut changed = SYNTHETIC_DESCRIPTOR.to_vec();
    changed[18] = 0x04;

    let error = registry
        .select(&fixture_descriptor(AdapterTransport::Usb, &changed))
        .unwrap_err();

    assert!(matches!(
        error,
        SelectionError::DescriptorFingerprintMismatch { .. }
    ));
}

#[test]
fn transport_is_part_of_exact_adapter_identity() {
    let registry = SupportRegistry::from_qualified_definitions([
        fixture_definition("synthetic-usb-fixture", AdapterTransport::Usb),
        fixture_definition(
            "synthetic-classic-bluetooth-fixture",
            AdapterTransport::BluetoothClassic,
        ),
    ]);

    let selected = registry
        .select(&fixture_descriptor(
            AdapterTransport::BluetoothClassic,
            SYNTHETIC_DESCRIPTOR,
        ))
        .unwrap();
    assert_eq!(selected.model(), "synthetic-classic-bluetooth-fixture");

    let error = registry
        .select(&fixture_descriptor(
            AdapterTransport::BluetoothLowEnergy,
            SYNTHETIC_DESCRIPTOR,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        SelectionError::UnsupportedModelTransport { .. }
    ));
}

#[test]
fn empty_and_ambiguous_registries_fail_closed() {
    let descriptor =
        fixture_descriptor(AdapterTransport::Usb, SYNTHETIC_DESCRIPTOR);
    assert!(matches!(
        SupportRegistry::empty().select(&descriptor),
        Err(SelectionError::UnsupportedModelTransport { .. })
    ));

    let definition =
        fixture_definition("synthetic-usb-fixture", AdapterTransport::Usb);
    let ambiguous = SupportRegistry::from_qualified_definitions([
        definition.clone(),
        definition,
    ]);
    assert!(matches!(
        ambiguous.select(&descriptor),
        Err(SelectionError::AmbiguousAdapter { match_count: 2, .. })
    ));
}

#[test]
fn policy_compilation_is_capability_gated() {
    let unsupported_input = [OneToOneMapping::new(
        ControlCode::keyboard(0x07),
        ControlCode::keyboard(0x14),
    )];
    assert!(matches!(
        compile_policy(
            &source_capabilities(),
            &destination_capabilities(),
            &unsupported_input,
            PolicyLimits::default(),
        ),
        Err(CompilePolicyError::UnsupportedInput { index: 0, .. })
    ));

    let unsupported_output = [OneToOneMapping::new(
        ControlCode::keyboard(0x04),
        ControlCode::consumer(0x00e9),
    )];
    assert!(matches!(
        compile_policy(
            &source_capabilities(),
            &destination_capabilities(),
            &unsupported_output,
            PolicyLimits::default(),
        ),
        Err(CompilePolicyError::UnsupportedOutput { index: 0, .. })
    ));
}

#[test]
fn one_to_one_conflicts_are_rejected() {
    let duplicate_input = [
        OneToOneMapping::new(
            ControlCode::keyboard(0x04),
            ControlCode::keyboard(0x14),
        ),
        OneToOneMapping::new(
            ControlCode::keyboard(0x04),
            ControlCode::keyboard(0x1a),
        ),
    ];
    assert!(matches!(
        compile_policy(
            &source_capabilities(),
            &destination_capabilities(),
            &duplicate_input,
            PolicyLimits::default(),
        ),
        Err(CompilePolicyError::InputConflict {
            first_index: 0,
            second_index: 1,
            ..
        })
    ));

    let duplicate_output = [
        OneToOneMapping::new(
            ControlCode::keyboard(0x04),
            ControlCode::keyboard(0x14),
        ),
        OneToOneMapping::new(
            ControlCode::keyboard(0x05),
            ControlCode::keyboard(0x14),
        ),
    ];
    assert!(matches!(
        compile_policy(
            &source_capabilities(),
            &destination_capabilities(),
            &duplicate_output,
            PolicyLimits::default(),
        ),
        Err(CompilePolicyError::OutputConflict {
            first_index: 0,
            second_index: 1,
            ..
        })
    ));
}

#[test]
fn compiled_operations_are_golden_and_bounded() {
    let mappings = [
        OneToOneMapping::new(
            ControlCode::keyboard(0x04),
            ControlCode::keyboard(0x14),
        ),
        OneToOneMapping::new(
            ControlCode::keyboard(0x05),
            ControlCode::keyboard(0x1a),
        ),
    ];
    let policy = compile_policy(
        &source_capabilities(),
        &destination_capabilities(),
        &mappings,
        PolicyLimits::new(2).unwrap(),
    )
    .unwrap();

    assert_eq!(
        policy.operations(),
        [
            DriverOperation::Remap {
                input: ControlCode::keyboard(0x04),
                output: ControlCode::keyboard(0x14),
            },
            DriverOperation::Remap {
                input: ControlCode::keyboard(0x05),
                output: ControlCode::keyboard(0x1a),
            },
        ]
    );
    assert_eq!(policy.operation_bound(), 2);

    let too_many = [
        mappings[0],
        mappings[1],
        OneToOneMapping::new(
            ControlCode::keyboard(0x06),
            ControlCode::keyboard(0x08),
        ),
    ];
    assert_eq!(
        compile_policy(
            &source_capabilities(),
            &destination_capabilities(),
            &too_many,
            PolicyLimits::new(2).unwrap(),
        ),
        Err(CompilePolicyError::OperationLimitExceeded {
            actual: 3,
            maximum: 2,
        })
    );
    assert!(PolicyLimits::new(MAX_DRIVER_POLICY_OPERATIONS + 1).is_err());
}
