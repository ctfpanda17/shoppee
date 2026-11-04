use shoppee_core::browser_bridge::{
    Broker, Catalog, Group, Kind, OptionItem, Outcome, Report, Selection,
};
fn catalog() -> Catalog {
    Catalog {
        canonical_url: "https://shopee.tw/product/123/456".into(),
        title: "Fixture product".into(),
        groups: vec![Group {
            name: "規格".into(),
            options: vec![
                OptionItem {
                    label: "T118".into(),
                    disabled: false,
                    selected: true,
                },
                OptionItem {
                    label: "PS5".into(),
                    disabled: true,
                    selected: false,
                },
            ],
        }],
        unit_price_minor: Some(10000),
        stock: Some(3),
        shipping_blocked: false,
    }
}
fn choices() -> Vec<Selection> {
    vec![Selection {
        group: "規格".into(),
        option: "T118".into(),
    }]
}
#[tokio::test]
async fn inspection_lease_is_single_use_and_stale_reports_are_rejected() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Inspect, catalog().canonical_url, vec![], 1, 0)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), broker.claim())
            .await
            .is_err()
    );
    assert!(broker
        .report(Report {
            command_id: cmd.id + 1,
            outcome: None,
            error: Some("stale".into())
        })
        .await
        .is_err());
    broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: catalog(),
                checkout_opened: false,
                payment_sent: false,
            }),
            error: None,
        })
        .await
        .unwrap();
    assert!(!task.await.unwrap().unwrap().checkout_opened);
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: None,
            error: Some("duplicate".into())
        })
        .await
        .is_err());
}
#[tokio::test]
async fn checkout_receipt_must_match_budget_selection_stock_and_no_payment() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Checkout, catalog().canonical_url, choices(), 2, 20000)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    let mut bad = catalog();
    bad.shipping_blocked = true;
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: bad,
                checkout_opened: true,
                payment_sent: false
            }),
            error: None
        })
        .await
        .is_err());
    let mut bad = catalog();
    bad.unit_price_minor = Some(10001);
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: bad,
                checkout_opened: true,
                payment_sent: false
            }),
            error: None
        })
        .await
        .is_err());
    let mut bad = catalog();
    bad.groups[0].options[0].selected = false;
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: bad,
                checkout_opened: true,
                payment_sent: false
            }),
            error: None
        })
        .await
        .is_err());
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: catalog(),
                checkout_opened: true,
                payment_sent: true
            }),
            error: None
        })
        .await
        .is_err());
    broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(Outcome {
                catalog: catalog(),
                checkout_opened: true,
                payment_sent: false,
            }),
            error: None,
        })
        .await
        .unwrap();
    assert!(task.await.unwrap().unwrap().checkout_opened);
}
#[tokio::test]
async fn cancellation_invalidates_a_leased_command() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Inspect, catalog().canonical_url, vec![], 1, 0)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    broker.cancel().await;
    assert!(task.await.unwrap().is_err());
    assert!(broker
        .report(Report {
            command_id: cmd.id,
            outcome: None,
            error: Some("late".into())
        })
        .await
        .is_err());
}
#[test]
fn catalog_rejects_disabled_duplicate_or_mismatched_options() {
    let mut c = catalog();
    assert!(c
        .validate_selection(&[Selection {
            group: "規格".into(),
            option: "PS5".into()
        }])
        .is_err());
    assert!(c.validate_selection(&[]).is_err());
    assert!(c.validate("https://shopee.tw/product/123/999").is_err());
    c.groups.push(c.groups[0].clone());
    assert!(c.validate(&c.canonical_url).is_err());
}
