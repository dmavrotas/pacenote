//! Benchmark tasks: everyday engineering requests against `bench/fixture`.
//!
//! The fixture is bug-free; each task injects its own bug into a fresh copy.
//! Tasks are split into **dev** (used while improving pacenote) and
//! **held-out** (written before the rewrite-grounding work started, scored
//! but never tuned against).

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Split {
    Dev,
    Holdout,
    All,
}

/// A string replacement that plants a bug.
pub struct Bug {
    pub file: &'static str,
    pub from: &'static str,
    pub to: &'static str,
}

pub struct Task {
    pub name: &'static str,
    pub split: Split,
    pub bugs: &'static [&'static Bug],
    /// unittest targets that must pass afterwards
    pub verify: &'static [&'static str],
    /// Written to `logs/app.log` when present.
    pub log: Option<String>,
    /// How the request is often typed.
    pub vague: String,
    /// The same request after pacenote's lint advice: where, and what done
    /// looks like, never how to fix it.
    pub specific: String,
}

pub const ALL_TESTS: &[&str] = &[
    "tests.test_pagination",
    "tests.test_pricing",
    "tests.test_inventory",
    "tests.test_users",
    "tests.test_report",
];

pub const PAGINATION: Bug = Bug {
    file: "shop/pagination.py",
    from: "start = (page - 1) * size\n",
    to: "start = (page - 1) * size + 1\n",
};
pub const PRICING: Bug = Bug {
    file: "shop/pricing.py",
    from: "return subtotal - subtotal * percent / 100\n",
    to: "return subtotal - subtotal * percent\n",
};
pub const LOW_STOCK: Bug = Bug {
    file: "shop/inventory.py",
    from: "if qty < threshold",
    to: "if qty > threshold",
};
pub const EMAIL_TLD: Bug = Bug {
    file: "shop/users.py",
    from: r"\.[a-z]{2,}$",
    to: r"\.[a-z]{4,}$",
};
pub const DISPLAY_NAME: Bug = Bug {
    file: "shop/users.py",
    from: r#"return f"{first} {last[:1]}." if last else first"#,
    to: r#"return f"{first} {last[0]}.""#,
};
pub const TOP_PRODUCTS: Bug = Bug {
    file: "shop/report.py",
    from: "key=lambda kv: (-kv[1], kv[0])",
    to: "key=lambda kv: (kv[1], kv[0])",
};

pub fn tasks() -> Vec<Task> {
    let checkout_log = app_log(
        14,
        217,
        "2026-10-30T14:03:37.412Z ERROR request_id=req-5f3a2c method=POST path=/api/checkout status=500\n\
         Traceback (most recent call last):\n  \
         File \"app/handlers/checkout.py\", line 41, in post\n    \
         total = order_total(cart.lines, discount_percent=coupon.percent)\n  \
         File \"shop/pricing.py\", line 25, in order_total\n    \
         raise ValueError(\"total cannot be negative\")\n\
         ValueError: total cannot be negative",
    );
    let profile_log = app_log(
        9,
        143,
        "2026-10-30T09:02:23.118Z ERROR request_id=req-9b21e7 method=GET path=/users/42/profile status=500\n\
         Traceback (most recent call last):\n  \
         File \"app/handlers/profile.py\", line 18, in get\n    \
         title = display_name(user)\n  \
         File \"shop/users.py\", line 27, in display_name\n    \
         return f\"{first} {last[0]}.\"\n\
         IndexError: string index out of range",
    );

    vec![
        // ---------------------------------------------------------------- dev
        Task {
            name: "failing-test",
            split: Split::Dev,
            bugs: &[&PAGINATION],
            verify: &["tests.test_pagination"],
            log: None,
            vague: "pagination is broken, can you fix it".into(),
            specific: "`paginate()` in shop/pagination.py returns the wrong items: page 1 should \
                       be [1, 2, 3] for ITEMS 1..10 with size 3. Fix it so \
                       `python3 -m unittest tests.test_pagination` passes."
                .into(),
        },
        Task {
            name: "production-error",
            split: Split::Dev,
            bugs: &[&PRICING],
            verify: &["tests.test_pricing"],
            vague: format!(
                "checkout is failing in production, here are the logs:\n\n{checkout_log}\n\nfix it"
            ),
            log: Some(checkout_log),
            specific: "Checkout raises `ValueError: total cannot be negative` from order_total() \
                       in shop/pricing.py when a 10% discount is applied (first ERROR in \
                       logs/app.log). Fix the cause so `python3 -m unittest tests.test_pricing` \
                       passes."
                .into(),
        },
        Task {
            name: "red-ci",
            split: Split::Dev,
            bugs: &[&PAGINATION, &PRICING],
            verify: ALL_TESTS,
            log: None,
            vague: "the tests are failing, fix them".into(),
            specific: "`python3 -m unittest` fails in tests/test_pagination.py and \
                       tests/test_pricing.py. Fix the bugs in shop/ (not the tests) so the whole \
                       suite passes."
                .into(),
        },
        // ----------------------------------------------------------- held-out
        Task {
            name: "stock-report",
            split: Split::Holdout,
            bugs: &[&LOW_STOCK],
            verify: &["tests.test_inventory"],
            log: None,
            vague: "the low stock report is wrong, fix it".into(),
            specific: "`Inventory.low_stock()` in shop/inventory.py returns well-stocked SKUs \
                       instead of the ones below the threshold. Fix it so \
                       `python3 -m unittest tests.test_inventory` passes."
                .into(),
        },
        Task {
            name: "signup-email",
            split: Split::Holdout,
            bugs: &[&EMAIL_TLD],
            verify: &["tests.test_users"],
            log: None,
            vague: "users say signup rejects their email address, can you fix it".into(),
            specific: "`validate_user()` in shop/users.py rejects valid emails such as \
                       ada@example.com. Fix it so `python3 -m unittest tests.test_users` passes."
                .into(),
        },
        Task {
            name: "top-products",
            split: Split::Holdout,
            bugs: &[&TOP_PRODUCTS],
            verify: &["tests.test_report"],
            log: None,
            vague: "the top products widget shows the wrong products, fix it".into(),
            specific: "`top_products()` in shop/report.py returns the least-sold SKUs instead of \
                       the best sellers. Fix it so `python3 -m unittest tests.test_report` passes."
                .into(),
        },
        Task {
            name: "red-ci-2",
            split: Split::Holdout,
            bugs: &[&LOW_STOCK, &EMAIL_TLD],
            verify: ALL_TESTS,
            log: None,
            vague: "CI is red, please fix".into(),
            specific: "`python3 -m unittest` fails in tests/test_inventory.py and \
                       tests/test_users.py. Fix the bugs in shop/ (not the tests) so the whole \
                       suite passes."
                .into(),
        },
        Task {
            name: "profile-crash",
            split: Split::Holdout,
            bugs: &[&DISPLAY_NAME],
            verify: &["tests.test_users"],
            vague: format!(
                "profile pages are crashing in production, logs below\n\n{profile_log}\n\nfix it"
            ),
            log: Some(profile_log),
            specific: "Profile pages raise `IndexError: string index out of range` from \
                       display_name() in shop/users.py for single-word names like \"Plato\" \
                       (first ERROR in logs/app.log). Fix it so \
                       `python3 -m unittest tests.test_users` passes."
                .into(),
        },
    ]
}

/// A realistic ~35 KB application log for `hour`, with `error` inserted
/// after line `at`.
pub fn app_log(hour: u32, at: u32, error: &str) -> String {
    let mut log = String::new();
    let paths = [
        "/api/products",
        "/api/cart",
        "/api/users/me",
        "/healthz",
        "/api/orders",
    ];
    for i in 0..400u32 {
        let (min, sec) = (i / 60, i % 60);
        let path = paths[(i as usize * 7) % paths.len()];
        let _ = writeln!(
            log,
            "2026-10-30T{hour:02}:{min:02}:{sec:02}.{:03}Z INFO  request_id=req-{:06x} method=GET path={path} status=200 duration_ms={}",
            (i * 37) % 1000,
            i * 2_654_435 % 0xff_ffff,
            5 + (i * 13) % 120
        );
        if i == at {
            let _ = writeln!(log, "{error}");
        }
    }
    log
}
