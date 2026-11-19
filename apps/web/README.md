# Shirori local Test Lab

A buildless HTML/CSS/JavaScript interface served by the Rust API. No npm install, bundler, external fonts, or frontend runtime dependencies.

From the repository root:

```bash
podman compose -f infrastructure/docker/compose.yaml up -d
# Configure .env from .env.example if needed, with SHIRORI_DEV_AUTH=true.
cargo run -p shirori-api -- seed database/seed/dev-seed.json
cargo run -p shirori-api
```

Open **http://127.0.0.1:8080/**. Assets are embedded at compile time, so restart `cargo run` after editing frontend files. The web routes are only registered when development authentication is enabled. This is a local testing tool; all actions use the actual API and persist to the development database.

## Try a reader's day

1. In Test settings, choose **Create a fresh reader** if the default Alice has existing history.
2. Read through Daily dose. Displaying each dose card records it as read. Save a card, open its sources, or expand the explanation. Arrow keys and horizontal swipes navigate between cards.
3. Finish the dose. Discover becomes available from the completion screen. Use **Mark as read** on Discover cards; loading a page alone only records an impression.
4. Open Saved ideas. Saves survive the no-repeat rule and show later corrections/withdrawals.
5. Change the simulated date in Test settings to test future doses. Change the reader name to test separate histories. Blank date uses the reader's time zone.
6. Preferences support time zone, consent, JSON export, and deletion of the selected test reader.

## Try the editorial flow

1. Open Editorial desk → Works to inspect or add a source record.
2. Cards → Write a card creates a draft. Supply the evidence label and limitations where required.
3. Preview / revise creates a new draft revision; Publish latest applies the real publication gate. Validation errors are displayed in the page.
4. To create a Connection card, add concepts, tag the ordinary cards at creation, propose and approve a matching relation, and select two published cards from different fields.
5. Withdrawing a card requires a reason and also withdraws dependent connections.

The editor identity is always `lab_editor`, separate from the selected reader. Works, concepts, relations, and feed registration use editable JSON forms for a small implementation; card writing uses labelled fields.

## Try ingestion

Register a feed under Content pipeline. Set rights and enable it deliberately. Start the worker in another terminal:

```bash
cargo run -p shirori-ingestion
```

Poll now queues a job. Refresh Sources, Review, and Jobs to see progress. Promote a review item into a source work, then write a card in the editorial desk. Duplicate promotion requires confirmation. Changed items can be acknowledged after review; dead jobs can be retried. The screen displays up to 100 review items/jobs per request and does not auto-poll.

## Verification

`node --check apps/web/app.js` checks JavaScript syntax. Backend tests include verifying that HTML/JS/CSS are served with correct content types only in development mode. Chrome smoke tests exercise the reader journey, editor draft/revision/publication/withdrawal flow, pipeline screens, and mobile layout.
