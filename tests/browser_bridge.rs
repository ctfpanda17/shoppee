use shoppee_core::browser_bridge::{
    Broker, CartSummary, Catalog, Group, Kind, OptionItem, Outcome, Report, Selection,
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
                cart: None,
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
fn receipt() -> Outcome {
    Outcome {
        catalog: catalog(),
        checkout_opened: true,
        payment_sent: false,
        cart: Some(CartSummary {
            total_minor: 20000,
            quantity: 2,
            item_count: 1,
        }),
    }
}
#[tokio::test]
async fn checkout_receipt_must_match_cart_budget_selection_stock_and_no_payment() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Checkout, catalog().canonical_url, choices(), 2, 20000)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    let mut shipping = receipt();
    shipping.catalog.shipping_blocked = true;
    let mut over = receipt();
    over.cart.as_mut().unwrap().total_minor = 20001;
    let mut selection = receipt();
    selection.catalog.groups[0].options[0].selected = false;
    let mut stock = receipt();
    stock.catalog.stock = Some(1);
    let mut payment = receipt();
    payment.payment_sent = true;
    let mut missing = receipt();
    missing.cart = None;
    let mut quantity = receipt();
    quantity.cart.as_mut().unwrap().quantity = 1;
    let mut count = receipt();
    count.cart.as_mut().unwrap().item_count = 3;
    for outcome in [
        shipping, over, selection, stock, payment, missing, quantity, count,
    ] {
        assert!(broker
            .report(Report {
                command_id: cmd.id,
                outcome: Some(outcome),
                error: None
            })
            .await
            .is_err());
    }
    broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(receipt()),
            error: None,
        })
        .await
        .unwrap();
    assert_eq!(
        task.await.unwrap().unwrap().cart.unwrap().total_minor,
        20000
    );
}
#[tokio::test]
async fn checkout_budget_uses_discounted_cart_total_instead_of_reference_unit_price() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Checkout, catalog().canonical_url, choices(), 2, 20000)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    let mut out = receipt();
    out.catalog.unit_price_minor = Some(1000000);
    out.cart.as_mut().unwrap().total_minor = 19999;
    broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(out),
            error: None,
        })
        .await
        .unwrap();
    assert_eq!(
        task.await.unwrap().unwrap().cart.unwrap().total_minor,
        19999
    );
}
#[tokio::test]
async fn explicit_zero_cart_total_and_unknown_reference_unit_price_are_valid() {
    let broker = Broker::default();
    let b = broker.clone();
    let task = tokio::spawn(async move {
        b.dispatch(Kind::Checkout, catalog().canonical_url, choices(), 2, 20000)
            .await
    });
    let cmd = broker.claim().await.unwrap();
    let mut out = receipt();
    out.catalog.unit_price_minor = None;
    out.cart.as_mut().unwrap().total_minor = 0;
    out.cart.as_mut().unwrap().item_count = 2;
    broker
        .report(Report {
            command_id: cmd.id,
            outcome: Some(out),
            error: None,
        })
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap().cart.unwrap().total_minor, 0);
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
    c.unit_price_minor = Some(0);
    assert!(c.validate(&c.canonical_url).is_ok());
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
