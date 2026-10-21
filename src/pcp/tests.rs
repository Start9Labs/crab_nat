use bytes::BytesMut;

use super::*;

#[test]
fn test_validate_base_response_too_short() {
    // Less than 24 bytes should return InvalidResponse
    let mut bb = BytesMut::from(&[0u8; 16][..]);
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::TooFewBytes {
            received: 16,
            expected: HEADER_SIZE
        })
    ));
}

#[test]
fn test_validate_base_response_invalid_length() {
    // More than 24 bytes but not a multiple of 4 bytes should return InvalidResponse
    let mut bb = BytesMut::from(&[0u8; 27][..]);
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::ResponseNotMultipleOfFour(27))
    ));
}

#[test]
fn test_validate_base_response_unsupported_version() {
    // Correct length, but version is not required PCP version 2
    let mut bb = BytesMut::with_capacity(HEADER_SIZE);
    bb.put_u8(0xFF); // version (invalid)
    bb.put_bytes(0u8, 23); // rest of header unchecked
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::UnknownVersion(0xFF))
    ));
}

#[test]
fn test_validate_base_response_unset_r_bit() {
    // Correct length and version but R bit is not set
    let mut bb = BytesMut::with_capacity(HEADER_SIZE);
    bb.put_u8(VersionCode::Pcp as u8);
    bb.put_u8(0x01); // opcode with R MSb unset
    bb.put_bytes(0, 22); // rest of header unchecked
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::ResponseBitNotSet(0x01))
    ));
}

#[test]
fn test_validate_base_response_success() {
    // 24 bytes, correct version, opcode, result code = Success
    let mut bb = BytesMut::with_capacity(HEADER_SIZE);
    bb.put_u8(VersionCode::Pcp as u8);
    bb.put_u8(0x80 | OperationCode::Map as u8); // opcode with R MSb set
    bb.put_u8(0x00); // reserved
    bb.put_u8(ResultCode::Success as u8);
    bb.put_u32(10); // lifetime
    bb.put_u32(20); // epoch
    bb.put_bytes(0, 12); // reserved
    let res = validate_base_response(&mut bb, OperationCode::Map).unwrap();
    assert_eq!(res.lifetime_seconds, 10);
    assert_eq!(res.gateway_epoch_seconds, 20);
}

#[test]
fn test_validate_base_response_natpmp_version_full_response() {
    // A ≥24-byte response with version 0 (NAT-PMP) should return UnsupportedVersion(NatPmp).
    let mut bb = BytesMut::with_capacity(HEADER_SIZE);
    bb.put_u8(VersionCode::NatPmp as u8); // version 0
    bb.put_bytes(0, 23); // rest of header
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(
        matches!(err, Failure::UnsupportedVersion(VersionCode::NatPmp)),
        "Expected UnsupportedVersion(NatPmp) for NAT-PMP version in full response, got: {err:?}"
    );
}

#[test]
fn test_validate_base_response_short_natpmp_unsupported_version() {
    // A short response (< 24 bytes) with NAT-PMP version and UnsupportedVersion result code.
    // This is the standard NAT-PMP server rejection path.
    let mut bb = BytesMut::with_capacity(4);
    bb.put_u8(VersionCode::NatPmp as u8); // version 0
    bb.put_u8(0x80); // opcode with R bit set
    bb.put_u8(0x00); // first byte of NAT-PMP u16 result code / PCP reserved byte
    bb.put_u8(ResultCode::UnsupportedVersion as u8); // PCP result code byte / NAT-PMP result code LSB
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::UnsupportedVersion(VersionCode::NatPmp)
    ));
}

/// Ensure all error result codes are correctly mapped to Failure variants, including additional failure context.
#[test]
fn test_validate_base_response_error_result_codes() {
    /// Helper to build a 24-byte PCP response with the given result code and lifetime.
    fn make_pcp_response(result_code: ResultCode, lifetime: u32) -> BytesMut {
        let mut bb = BytesMut::with_capacity(HEADER_SIZE);
        bb.put_u8(VersionCode::Pcp as u8);
        bb.put_u8(0x80 | OperationCode::Map as u8);
        bb.put_u8(0x00); // reserved
        bb.put_u8(result_code as u8);
        bb.put_u32(lifetime);
        bb.put_u32(100); // epoch
        bb.put_bytes(0, 12); // reserved
        bb
    }

    // `NotAuthorized` includes the lifetime as retry-after in seconds.
    let mut bb = make_pcp_response(ResultCode::NotAuthorized, 30);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::NotAuthorized(30)
    ));

    let mut bb = make_pcp_response(ResultCode::MalformedRequest, 0);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::MalformedRequest
    ));

    // `NetworkFailure` includes the lifetime as retry-after in seconds.
    let mut bb = make_pcp_response(ResultCode::NetworkFailure, 60);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::NetworkFailure(60)
    ));

    // `NoResources` includes the lifetime as retry-after in seconds.
    let mut bb = make_pcp_response(ResultCode::NoResources, 120);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::NoResources(120)
    ));

    let mut bb = make_pcp_response(ResultCode::UnsupportedProtocol, 0);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::UnsupportedProtocol
    ));

    let mut bb = make_pcp_response(ResultCode::AddressMismatch, 0);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::AddressMismatch
    ));

    let mut bb = make_pcp_response(ResultCode::ExcessiveRemotePeers, 0);
    assert!(matches!(
        validate_base_response(&mut bb, OperationCode::Map).unwrap_err(),
        Failure::ExcessiveRemotePeers
    ));
}

#[test]
fn test_validate_base_response_wrong_opcode() {
    let mut bb = BytesMut::with_capacity(HEADER_SIZE);
    bb.put_u8(VersionCode::Pcp as u8);
    bb.put_u8(0x80 | OperationCode::Peer as u8); // Peer opcode
    bb.put_u8(0x00);
    bb.put_u8(ResultCode::Success as u8);
    bb.put_u32(10);
    bb.put_u32(20);
    bb.put_bytes(0, 12);
    let err = validate_base_response(&mut bb, OperationCode::Map).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::IncorrectOpcode {
            received: OperationCode::Peer,
            expected: OperationCode::Map
        })
    ));
}

#[test]
fn test_protocol_to_byte() {
    // Per IANA protocol numbers: TCP=6, UDP=17
    assert_eq!(protocol_to_byte(InternetProtocol::Tcp), 6);
    assert_eq!(protocol_to_byte(InternetProtocol::Udp), 17);
}

#[test]
fn test_fixed_size_addr_v4() {
    let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let v6 = fixed_size_addr(ip);
    // IPv4-mapped IPv6: ::ffff:192.168.1.1
    assert_eq!(v6, Ipv4Addr::new(192, 168, 1, 1).to_ipv6_mapped());
    assert_eq!(v6.to_ipv4_mapped(), Some(Ipv4Addr::new(192, 168, 1, 1)));
}

#[test]
fn test_fixed_size_addr_v6() {
    let v6_orig = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let ip = IpAddr::V6(v6_orig);
    assert_eq!(fixed_size_addr(ip), v6_orig);
}

#[test]
fn test_read_ip6_addr() {
    let addr = Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1);
    let octets = addr.octets();
    let mut reader = &octets[..];
    let parsed = read_ip6_addr(&mut reader);
    assert_eq!(parsed, addr);
    assert_eq!(reader.remaining(), 0);
}

#[test]
fn test_validate_nonce_success() {
    let nonce: Nonce = [0xDEADBEEF, 0x00C0FFEE, 0x12345678];
    let mut buf = BytesMut::with_capacity(12);
    buf.put_u32(nonce[0]);
    buf.put_u32(nonce[1]);
    buf.put_u32(nonce[2]);
    let mut reader = &buf[..];
    assert!(validate_nonce(&mut reader, nonce).is_ok());
}

#[test]
fn test_validate_nonce_mismatch() {
    let nonce: Nonce = [0xDEADBEEF, 0x00C0FFEE, 0x12345678];
    let mut buf = BytesMut::with_capacity(12);
    buf.put_u32(0x11111111); // wrong
    buf.put_u32(nonce[1]);
    buf.put_u32(nonce[2]);
    let mut reader = &buf[..];
    assert!(matches!(
        validate_nonce(&mut reader, nonce).unwrap_err(),
        Failure::Nonce
    ));
}

#[test]
fn test_validate_port_success() {
    let mut buf = BytesMut::with_capacity(2);
    buf.put_u16(8080);
    let mut reader = &buf[..];
    assert!(validate_port(&mut reader, 8080).is_ok());
}

#[test]
fn test_validate_port_mismatch() {
    let mut buf = BytesMut::with_capacity(2);
    buf.put_u16(9090);
    let mut reader = &buf[..];
    assert_eq!(validate_port(&mut reader, 8080).unwrap_err(), 9090);
}

#[test]
fn test_validate_protocol_tcp() {
    let mut buf = BytesMut::with_capacity(1);
    buf.put_u8(6); // TCP per IANA
    let mut reader = &buf[..];
    assert!(validate_protocol(&mut reader, Some(InternetProtocol::Tcp)).is_ok());
}

#[test]
fn test_validate_protocol_udp() {
    let mut buf = BytesMut::with_capacity(1);
    buf.put_u8(17); // UDP per IANA
    let mut reader = &buf[..];
    assert!(validate_protocol(&mut reader, Some(InternetProtocol::Udp)).is_ok());
}

#[test]
fn test_validate_protocol_none_expects_zero() {
    // When protocol is None (all-protocols mapping), expect byte 0.
    let mut buf = BytesMut::with_capacity(1);
    buf.put_u8(0);
    let mut reader = &buf[..];
    assert!(validate_protocol(&mut reader, None).is_ok());
}

#[test]
fn test_validate_protocol_mismatch() {
    let mut buf = BytesMut::with_capacity(1);
    buf.put_u8(17); // UDP
    let mut reader = &buf[..];
    let err = validate_protocol(&mut reader, Some(InternetProtocol::Tcp)).unwrap_err();
    assert!(matches!(
        err,
        Failure::InvalidResponse(InvalidResponseKind::IncorrectProtocol {
            received: 17,
            expected: Some(InternetProtocol::Tcp)
        })
    ));
}

#[test]
fn test_response_to_opcode_map() {
    assert_eq!(
        response_to_opcode(0x80 | OperationCode::Map as u8).unwrap(),
        OperationCode::Map
    );
}

#[test]
fn test_response_to_opcode_peer() {
    assert_eq!(
        response_to_opcode(0x80 | OperationCode::Peer as u8).unwrap(),
        OperationCode::Peer
    );
}

#[test]
fn test_response_to_opcode_invalid() {
    // Opcode 0x7F with R bit set → invalid opcode
    assert!(response_to_opcode(0xFF).is_err());
}

#[test]
fn test_write_base_request_ipv4() {
    let client = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    let mut buf = [0u8; HEADER_SIZE];
    let mut bb = &mut buf[..];
    let suggested_ip = write_base_request(OperationCode::Map, client, None, &mut bb, 7200);

    // Version should be 2 (PCP)
    assert_eq!(buf[0], VersionCode::Pcp as u8);
    // Opcode should be Map (1) with R bit unset
    assert_eq!(buf[1], OperationCode::Map as u8);
    // Reserved (2 bytes)
    assert_eq!(buf[2], 0);
    assert_eq!(buf[3], 0);
    // Lifetime (4 bytes, big-endian)
    assert_eq!(u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]), 7200);
    // Client IP: IPv4-mapped IPv6 for 192.168.1.100
    let client_ip6 = Ipv4Addr::new(192, 168, 1, 100).to_ipv6_mapped();
    assert_eq!(&buf[8..24], &client_ip6.octets());
    // Suggested external IP should be IPv4-mapped unspecified when None
    assert_eq!(suggested_ip, Ipv4Addr::UNSPECIFIED.to_ipv6_mapped());
}

#[test]
fn test_pcp_option_wire_format() {
    for (data, expected) in [
        (vec![], vec![224, 0, 0, 0]),
        (vec![1], vec![224, 0, 0, 1, 1, 0, 0, 0]),
        (vec![1, 2], vec![224, 0, 0, 2, 1, 2, 0, 0]),
        (vec![1, 2, 3], vec![224, 0, 0, 3, 1, 2, 3, 0]),
        (vec![1, 2, 3, 4], vec![224, 0, 0, 4, 1, 2, 3, 4]),
    ] {
        let option = PcpOption { code: 224, data };
        let mut buf = [0xff; 8];
        let mut bb = &mut buf[..];
        option.encode(&mut bb).unwrap();
        let size = 8 - bb.len();
        assert_eq!(&buf[..size], expected);
        assert_eq!(parse_options(&expected).unwrap(), vec![option]);
    }
}

#[test]
fn test_parse_multiple_options() {
    assert_eq!(
        parse_options(&[224, 0, 0, 1, 7, 0, 0, 0, 200, 0, 0, 0]).unwrap(),
        vec![
            PcpOption {
                code: 224,
                data: vec![7]
            },
            PcpOption {
                code: 200,
                data: vec![]
            },
        ]
    );
    assert!(parse_options(&[]).unwrap().is_empty());
}

#[test]
fn test_parse_options_rejects_truncation() {
    for tail in [
        &[224][..],
        &[224, 0, 0, 10][..],
        &[224, 0, 0, 1, 7][..],
        &[200, 0, 0, 0, 224, 0, 0, 10][..],
    ] {
        assert!(matches!(
            parse_options(tail),
            Err(InvalidResponseKind::TruncatedOption)
        ));
    }
}

#[test]
fn test_option_size_limits() {
    let mut buf = [0; MAX_DATAGRAM_SIZE - 60];
    let option = PcpOption {
        code: 224,
        data: vec![7; 1036],
    };
    let mut bb = &mut buf[..];
    option.encode(&mut bb).unwrap();
    assert!(bb.is_empty());
    assert_eq!(&buf[..4], &[224, 0, 4, 12]);
    assert_eq!(parse_options(&buf).unwrap(), vec![option]);

    for length in [1037, usize::from(u16::MAX) + 1] {
        let option = PcpOption {
            code: 224,
            data: vec![7; length],
        };
        assert!(matches!(
            option.encode(&mut &mut buf[..]),
            Err(Failure::RequestTooLarge)
        ));
    }
}

#[tokio::test]
async fn test_aggregate_option_size_limit_before_socket_connect() {
    let options = vec![
        PcpOption {
            code: 224,
            data: vec![7; 520]
        };
        2
    ];
    let result = port_mapping(
        BaseMapRequest::new(
            GatewayAddress::IpV6("fe80::1".parse().unwrap(), None),
            Ipv6Addr::LOCALHOST.into(),
            InternetProtocol::Tcp,
            NonZeroU16::new(8080).unwrap(),
        ),
        None,
        None,
        PortMappingOptions::default(),
        &options,
    )
    .await;
    assert!(matches!(result, Err(Failure::RequestTooLarge)));
}

#[tokio::test]
async fn test_map_options_lifecycle_and_fallback() {
    use tokio::net::UdpSocket;

    for gateway in [
        GatewayAddress::IpV4(Ipv4Addr::LOCALHOST),
        GatewayAddress::IpV6(Ipv6Addr::LOCALHOST, Some(0)),
    ] {
        let socket = UdpSocket::bind((IpAddr::from(gateway), crate::GATEWAY_PORT))
            .await
            .unwrap();
        let request_options = vec![
            PcpOption {
                code: 224,
                data: b"example.com".to_vec(),
            },
            PcpOption {
                code: 200,
                data: vec![],
            },
        ];
        let response_options = vec![PcpOption {
            code: 201,
            data: vec![9],
        }];
        let timeout_config = TimeoutConfig {
            initial_timeout: Duration::from_secs(2),
            max_retries: 0,
            max_retry_timeout: None,
        };
        let options = PortMappingOptions {
            timeout_config: Some(timeout_config),
            ..Default::default()
        };
        let internal_port = NonZeroU16::new(8080).unwrap();
        let client = IpAddr::from(gateway);
        let server = async {
            let mut first_nonce = [0; 12];
            for step in 0..10 {
                let mut buf = [0; MAX_DATAGRAM_SIZE];
                let (n, peer) =
                    tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buf))
                        .await
                        .unwrap()
                        .unwrap();
                if step >= 8 {
                    assert_eq!(n, 12);
                    assert_eq!(&buf[..4], &[0, 2, 0, 0]);
                    assert_eq!(&buf[4..6], &[31, 144]);
                    assert_eq!(
                        &buf[8..12],
                        if step == 8 {
                            &[0, 0, 28, 32]
                        } else {
                            &[0, 0, 0, 0]
                        }
                    );
                    let lifetime = if step == 8 { 7200u32 } else { 0 };
                    let mut response = vec![0, 130, 0, 0, 0, 0, 0, 12, 31, 144];
                    response.extend_from_slice(if step == 8 { &[31, 144] } else { &[0, 0] });
                    response.extend_from_slice(&lifetime.to_be_bytes());
                    socket.send_to(&response, peer).await.unwrap();
                    continue;
                }
                assert_eq!(&buf[..4], &[2, 1, 0, 0]);
                assert_eq!(&buf[36..40], &[6, 0, 0, 0]);
                assert_eq!(&buf[40..42], &[31, 144]);
                if step < 5 {
                    assert_eq!(
                        &buf[60..n],
                        &[
                            224, 0, 0, 11, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c',
                            b'o', b'm', 0, 200, 0, 0, 0
                        ]
                    );
                    if step == 0 {
                        first_nonce.copy_from_slice(&buf[24..36]);
                    }
                    assert_eq!(&buf[24..36], &first_nonce);
                } else {
                    assert_eq!(n, 60);
                }
                let deleting = step == 3 || step == 4 || step == 6;
                assert_eq!(
                    &buf[4..8],
                    if deleting {
                        &[0, 0, 0, 0]
                    } else {
                        &[0, 0, 28, 32]
                    }
                );
                if deleting {
                    assert_eq!(&buf[42..44], &[0, 0]);
                }
                if step == 7 {
                    socket.send_to(&[0, 128, 0, 1], peer).await.unwrap();
                    continue;
                }
                let mut response = buf[..60].to_vec();
                response[1] = 0x81;
                response[8..24].fill(0);
                response[11] = 12;
                response[42..44].copy_from_slice(&[31, 144]);
                response[44..60]
                    .copy_from_slice(&Ipv4Addr::new(203, 0, 113, 7).to_ipv6_mapped().octets());
                match step {
                    0 => response.extend_from_slice(&[201, 0, 0, 1, 9, 0, 0, 0]),
                    1 => response.extend_from_slice(&[201, 0, 0, 10]),
                    2 => response.extend_from_slice(&[202, 0, 0, 0]),
                    3 => response[24] ^= 1,
                    _ => (),
                }
                socket.send_to(&response, peer).await.unwrap();
            }
        };
        let client = async {
            let mut mapping = port_mapping(
                BaseMapRequest::new(gateway, client, InternetProtocol::Tcp, internal_port),
                None,
                None,
                options,
                &request_options,
            )
            .await
            .unwrap();
            assert_eq!(mapping.gateway(), gateway);
            assert_eq!(mapping.response_options(), response_options);
            let expiration = mapping.expiration();
            assert!(matches!(
                mapping.renew().await,
                Err(crate::MappingFailure::Pcp(Failure::InvalidResponse(
                    InvalidResponseKind::TruncatedOption
                )))
            ));
            assert_eq!(mapping.expiration(), expiration);
            assert_eq!(mapping.response_options(), response_options);
            mapping.renew().await.unwrap();
            assert_eq!(
                mapping.response_options(),
                &[PcpOption {
                    code: 202,
                    data: vec![]
                }]
            );
            let (err, mapping) = mapping.try_drop().await.unwrap_err();
            assert!(matches!(err, crate::MappingFailure::Pcp(Failure::Nonce)));
            mapping.try_drop().await.unwrap();

            let mapping = PortMapping::new(
                gateway,
                client,
                InternetProtocol::Tcp,
                internal_port,
                options,
            )
            .await
            .unwrap();
            assert!(mapping.response_options().is_empty());
            mapping.try_drop().await.unwrap();

            let mapping = PortMapping::new(
                gateway,
                client,
                InternetProtocol::Tcp,
                internal_port,
                options,
            )
            .await
            .unwrap();
            assert!(matches!(mapping.mapping_type(), PortMappingType::NatPmp));
            assert!(mapping.response_options().is_empty());
            mapping.try_drop().await.unwrap();
        };
        tokio::join!(server, client);
    }
}

#[tokio::test]
async fn test_all_ports_preserves_requested_protocol() {
    let address = Ipv4Addr::new(127, 0, 0, 2);
    let socket = tokio::net::UdpSocket::bind((address, crate::GATEWAY_PORT))
        .await
        .unwrap();
    for (protocol, expected) in [
        (Some(InternetProtocol::Tcp), 6),
        (Some(InternetProtocol::Udp), 17),
        (None, 0),
    ] {
        let server = async {
            let mut buf = [0; MAX_DATAGRAM_SIZE];
            let (n, peer) =
                tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buf))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(n, 60);
            assert_eq!(buf[36], expected);
            assert_eq!(&buf[40..44], &[0; 4]);
            buf[1] = 0x81;
            buf[8..24].fill(0);
            socket.send_to(&buf[..n], peer).await.unwrap();
        };
        let client = port_mapping_all_ports(
            address.into(),
            address.into(),
            protocol,
            None,
            None,
            None,
            Some(TimeoutConfig {
                initial_timeout: Duration::from_secs(2),
                max_retries: 0,
                max_retry_timeout: None,
            }),
        );
        let (_, result) = tokio::join!(server, client);
        result.unwrap();
    }
}
