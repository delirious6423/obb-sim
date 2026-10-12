//! Person-held roles: the person may hold the general manager, owner and/or
//! commissioner seat. Everything goes through the public protocol only.

use host::api::{
    Action, Advance, Difficulty, LeagueView, NewGame, Overview, OwnerView, Preset, Request,
    Response, Roles, RulesView, Stage, Stance,
};
use host::{GameClient, LocalClient};

fn client(dir: &std::path::Path) -> LocalClient {
    LocalClient::start(host::calibration(), Some(dir.to_path_buf()))
}

fn ok(r: Response) -> String {
    match r {
        Response::Done(m) => m,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn refused(r: Response) -> String {
    match r {
        Response::Failed(e) => e,
        other => panic!("expected Failed, got {other:?}"),
    }
}

fn overview(c: &impl GameClient) -> Overview {
    match c.call(Request::Overview) {
        Response::Overview(o) => o,
        other => panic!("expected an overview, got {other:?}"),
    }
}

fn owner(c: &impl GameClient) -> OwnerView {
    match c.call(Request::OwnerDesk) {
        Response::Owner(o) => o,
        other => panic!("expected the owner's desk, got {other:?}"),
    }
}

fn league(c: &impl GameClient) -> LeagueView {
    match c.call(Request::LeagueOffice) {
        Response::League(l) => l,
        other => panic!("expected the league office, got {other:?}"),
    }
}

fn new_game(c: &impl GameClient, roles: Roles) {
    ok(c.call(Request::NewGame(NewGame {
        seed: 77,
        club: 4,
        difficulty: Difficulty::Preset(Preset::Veteran),
        burn_in_years: 2,
        roles,
    })));
}

fn play_season(c: &impl GameClient) {
    ok(c.call(Request::Advance(Advance::SimToEndOfSeason)));
    assert_eq!(overview(c).stage, Stage::Playoffs);
}

/// Run the draft to its end. Passes whenever the person is on the clock.
/// Returns how many times the person was on the clock.
fn run_draft(c: &impl GameClient) -> usize {
    let mut on_clock = 0;
    let mut rounds = 0;
    loop {
        assert!(rounds < 400, "the draft did not finish");
        rounds += 1;
        ok(c.call(Request::Advance(Advance::SimDraft)));
        let o = overview(c);
        if o.stage != Stage::Draft {
            return on_clock;
        }
        assert!(o.on_the_clock, "the draft stopped for no reason");
        on_clock += 1;
        ok(c.call(Request::Act(Action::PassPick)));
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sim-host-roles-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

/// (a) Owner and commissioner, not the GM: the season closes at an Offseason
/// stop where the person's owner and rule actions work, and the draft opens
/// once the person closes the books.
#[test]
fn owner_and_commissioner_run_the_offseason() {
    let dir = tempfile_dir("a");
    let c = client(&dir);
    let roles = Roles {
        gm: false,
        owner: true,
        commissioner: true,
    };
    new_game(&c, roles);
    assert_eq!(overview(&c).roles, roles);
    play_season(&c);

    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&c).stage, Stage::Offseason);

    let desk = owner(&c);
    assert!(desk.can_act);
    let price = (desk.ticket_range.0 + desk.ticket_range.1) / 2.0;
    let lg = league(&c);
    assert!(lg.can_act);
    let payroll = (lg.rules.payroll_floor + lg.rules.payroll_cap) / 2.0;
    // Rules may move at most 10% a year.
    let war = lg.rules.war_price * 1.05;

    ok(c.call(Request::Act(Action::SetBudget { payroll })));
    ok(c.call(Request::Act(Action::SetTicketPrice { price })));
    ok(c.call(Request::Act(Action::BuildStadium { amount: 1.0 })));
    ok(c.call(Request::Act(Action::SetStance {
        stance: Stance::Rebuild,
    })));
    let mut rules = lg.rules;
    rules.war_price = war;
    ok(c.call(Request::Act(Action::SetRules { rules })));

    let after = owner(&c);
    assert!(near(after.budget, payroll), "budget {}", after.budget);
    assert!(
        near(after.stadium.ticket, price),
        "ticket {}",
        after.stadium.ticket
    );
    assert_eq!(after.stance, Stance::Rebuild);
    assert!(near(league(&c).rules.war_price, war));

    // Close the books: the draft opens, and the person is never on the clock.
    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&c).stage, Stage::Draft);
    assert_eq!(run_draft(&c), 0);
    assert_eq!(overview(&c).stage, Stage::FreeAgency);
    ok(c.call(Request::Advance(Advance::StartNextSeason)));
    assert_eq!(overview(&c).stage, Stage::Season);
}

/// (b) A GM-only person cannot use the owner or commissioner actions, and the
/// owner's desk and league office are closed to them. The season closes
/// straight through to the draft.
#[test]
fn gm_only_cannot_use_owner_or_commissioner_actions() {
    let dir = tempfile_dir("b");
    let c = client(&dir);
    new_game(&c, Roles::default());
    play_season(&c);
    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&c).stage, Stage::Draft);

    assert!(refused(c.call(Request::OwnerDesk)).contains("role"));
    assert!(refused(c.call(Request::LeagueOffice)).contains("role"));
    assert!(refused(c.call(Request::Act(Action::SetBudget { payroll: 100.0 }))).contains("role"));
    assert!(refused(c.call(Request::Act(Action::SetTicketPrice { price: 20.0 }))).contains("role"));
    assert!(refused(c.call(Request::Act(Action::BuildStadium { amount: 1.0 }))).contains("role"));
    assert!(
        refused(c.call(Request::Act(Action::SetStance {
            stance: Stance::Contend,
        })))
        .contains("role")
    );
    let rules = RulesView {
        payroll_floor: 100.0,
        payroll_cap: 200.0,
        revenue_sharing: 0.2,
        war_price: 5.0,
        draft_rounds: 2,
        international_rounds: 0,
        economy: Default::default(),
    };
    assert!(refused(c.call(Request::Act(Action::SetRules { rules }))).contains("role"));
    // Nothing moved: the stage is still the draft.
    assert_eq!(overview(&c).stage, Stage::Draft);
}

/// (c) Owner only: the club's GM seat is played by the AI, so the draft runs
/// through the person's pick without stopping for the person.
#[test]
fn owner_only_leaves_the_draft_to_the_ai_gm() {
    let dir = tempfile_dir("c");
    let c = client(&dir);
    let roles = Roles {
        gm: false,
        owner: true,
        commissioner: false,
    };
    new_game(&c, roles);
    play_season(&c);

    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&c).stage, Stage::Offseason);
    assert!(owner(&c).can_act);
    refused(c.call(Request::LeagueOffice));

    ok(c.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&c).stage, Stage::Draft);
    assert!(!overview(&c).on_the_clock);
    assert_eq!(run_draft(&c), 0, "the draft stopped on the person's pick");
    assert_eq!(overview(&c).stage, Stage::FreeAgency);
}

/// (d) Save and reload keep the roles and the stop behaviour.
#[test]
fn saved_roles_and_stops_survive_a_reload() {
    let dir = tempfile_dir("d");
    let a = client(&dir);
    let roles = Roles {
        gm: true,
        owner: true,
        commissioner: true,
    };
    new_game(&a, roles);
    play_season(&a);
    ok(a.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&a).stage, Stage::Offseason);
    let lg = league(&a);
    let payroll = (lg.rules.payroll_floor + lg.rules.payroll_cap) / 2.0;
    ok(a.call(Request::Act(Action::SetBudget { payroll })));
    ok(a.call(Request::Save {
        slot: "roles".into(),
    }));
    drop(a);

    let b = client(&dir);
    ok(b.call(Request::Load {
        slot: "roles".into(),
    }));
    let o = overview(&b);
    assert_eq!(o.roles, roles);
    assert_eq!(o.stage, Stage::Offseason);
    assert!(near(owner(&b).budget, payroll));

    // Close this offseason. The person is the GM too, so the draft stops on
    // their pick.
    ok(b.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&b).stage, Stage::Draft);
    assert!(run_draft(&b) > 0, "the GM's pick never came up");
    assert_eq!(overview(&b).stage, Stage::FreeAgency);
    ok(b.call(Request::Advance(Advance::StartNextSeason)));
    // The next season still stops at the Offseason for the owner.
    play_season(&b);
    ok(b.call(Request::Advance(Advance::FinishSeason)));
    assert_eq!(overview(&b).stage, Stage::Offseason);
}

/// A roles set with none held is refused.
#[test]
fn a_person_must_hold_at_least_one_role() {
    let dir = tempfile_dir("f");
    let c = client(&dir);
    let none = Roles {
        gm: false,
        owner: false,
        commissioner: false,
    };
    let e = refused(c.call(Request::NewGame(NewGame {
        seed: 77,
        club: 4,
        difficulty: Difficulty::Preset(Preset::Veteran),
        burn_in_years: 2,
        roles: none,
    })));
    assert!(e.contains("role"), "{e}");
}

/// The owner's desk shows the lenders' limit (never below the floor), and a
/// budget at the highest allowed figure is accepted. The refusal above the
/// limit is tested in crates/world/tests/economy.rs.
#[test]
fn the_owner_sees_the_lenders_limit() {
    let dir = tempfile_dir("limit");
    let c = client(&dir);
    let roles = Roles {
        gm: false,
        owner: true,
        commissioner: true,
    };
    new_game(&c, roles);
    play_season(&c);
    ok(c.call(Request::Advance(Advance::FinishSeason)));

    let desk = owner(&c);
    let lg = league(&c);
    let (floor, cap) = (lg.rules.payroll_floor, lg.rules.payroll_cap);
    assert!(desk.budget_limit >= floor, "limit below the floor");
    // The highest budget the desk can offer: the lenders' limit or the cap.
    let top = desk.budget_limit.min(cap);
    assert!(top >= floor, "{top} below the floor");
    ok(c.call(Request::Act(Action::SetBudget { payroll: top })));
    assert!(near(owner(&c).budget, top));
}

/// A stadium built after the budget was set can lower the lenders' limit
/// below it. Closing the books must bring the budget back under the limit.
#[test]
fn a_budget_above_the_lenders_limit_is_brought_under_it_at_close() {
    let dir = tempfile_dir("close");
    let c = client(&dir);
    let roles = Roles {
        gm: false,
        owner: true,
        commissioner: true,
    };
    new_game(&c, roles);
    play_season(&c);
    ok(c.call(Request::Advance(Advance::FinishSeason)));

    let cap = league(&c).rules.payroll_cap;
    ok(c.call(Request::Act(Action::SetBudget { payroll: cap })));
    // Spend the club's cash (up to 200) so the lenders' room shrinks. The
    // build may not take the club below zero cash.
    let cash = owner(&c).cash;
    let amount = cash.min(200.0);
    assert!(amount > 0.0, "the club needs cash to build, has {cash}");
    ok(c.call(Request::Act(Action::BuildStadium { amount })));
    let before = owner(&c);
    assert!(before.budget_limit < cap, "the build should bind the limit");
    assert!(before.budget > before.budget_limit);

    ok(c.call(Request::Advance(Advance::FinishSeason)));
    let after = owner(&c);
    assert!(after.budget <= after.budget_limit + 1e-9, "{after:?}");
}
