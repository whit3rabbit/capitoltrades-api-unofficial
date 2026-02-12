CREATE TABLE IF NOT EXISTS assets (
    asset_id INTEGER PRIMARY KEY,
    asset_type TEXT NOT NULL,
    asset_ticker TEXT,
    instrument TEXT
);

CREATE TABLE IF NOT EXISTS issuers (
    issuer_id INTEGER PRIMARY KEY,
    state_id TEXT,
    c2iq TEXT,
    country TEXT,
    issuer_name TEXT NOT NULL,
    issuer_ticker TEXT,
    sector TEXT,
    enriched_at TEXT
);

CREATE TABLE IF NOT EXISTS politicians (
    politician_id TEXT PRIMARY KEY,
    state_id TEXT NOT NULL,
    party TEXT NOT NULL,
    party_other TEXT,
    district TEXT,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    nickname TEXT,
    middle_name TEXT,
    full_name TEXT,
    dob TEXT NOT NULL,
    gender TEXT NOT NULL,
    social_facebook TEXT,
    social_twitter TEXT,
    social_youtube TEXT,
    website TEXT,
    chamber TEXT NOT NULL,
    enriched_at TEXT
);

CREATE TABLE IF NOT EXISTS trades (
    tx_id INTEGER PRIMARY KEY,
    politician_id TEXT NOT NULL,
    asset_id INTEGER NOT NULL,
    issuer_id INTEGER NOT NULL,
    pub_date TEXT NOT NULL,
    filing_date TEXT NOT NULL,
    tx_date TEXT NOT NULL,
    tx_type TEXT NOT NULL,
    tx_type_extended TEXT,
    has_capital_gains INTEGER NOT NULL,
    owner TEXT NOT NULL,
    chamber TEXT NOT NULL,
    price REAL,
    size INTEGER,
    size_range_high INTEGER,
    size_range_low INTEGER,
    value INTEGER NOT NULL,
    filing_id INTEGER NOT NULL,
    filing_url TEXT NOT NULL,
    reporting_gap INTEGER NOT NULL,
    comment TEXT,
    enriched_at TEXT,
    trade_date_price REAL,
    current_price REAL,
    price_enriched_at TEXT,
    estimated_shares REAL,
    estimated_value REAL,
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trade_committees (
    tx_id INTEGER NOT NULL,
    committee TEXT NOT NULL,
    PRIMARY KEY (tx_id, committee),
    FOREIGN KEY (tx_id) REFERENCES trades(tx_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trade_labels (
    tx_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (tx_id, label),
    FOREIGN KEY (tx_id) REFERENCES trades(tx_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS politician_committees (
    politician_id TEXT NOT NULL,
    committee TEXT NOT NULL,
    PRIMARY KEY (politician_id, committee),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS politician_stats (
    politician_id TEXT PRIMARY KEY,
    date_last_traded TEXT,
    count_trades INTEGER NOT NULL,
    count_issuers INTEGER NOT NULL,
    volume INTEGER NOT NULL,
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS issuer_stats (
    issuer_id INTEGER PRIMARY KEY,
    count_trades INTEGER NOT NULL,
    count_politicians INTEGER NOT NULL,
    volume INTEGER NOT NULL,
    date_last_traded TEXT NOT NULL,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS issuer_performance (
    issuer_id INTEGER PRIMARY KEY,
    mcap INTEGER NOT NULL,
    trailing1 REAL NOT NULL,
    trailing1_change REAL NOT NULL,
    trailing7 REAL NOT NULL,
    trailing7_change REAL NOT NULL,
    trailing30 REAL NOT NULL,
    trailing30_change REAL NOT NULL,
    trailing90 REAL NOT NULL,
    trailing90_change REAL NOT NULL,
    trailing365 REAL NOT NULL,
    trailing365_change REAL NOT NULL,
    wtd REAL NOT NULL,
    wtd_change REAL NOT NULL,
    mtd REAL NOT NULL,
    mtd_change REAL NOT NULL,
    qtd REAL NOT NULL,
    qtd_change REAL NOT NULL,
    ytd REAL NOT NULL,
    ytd_change REAL NOT NULL,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS issuer_eod_prices (
    issuer_id INTEGER NOT NULL,
    price_date TEXT NOT NULL,
    price REAL NOT NULL,
    PRIMARY KEY (issuer_id, price_date),
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS positions (
    politician_id TEXT NOT NULL,
    issuer_ticker TEXT NOT NULL,
    shares_held REAL NOT NULL,
    cost_basis REAL NOT NULL,
    realized_pnl REAL NOT NULL DEFAULT 0.0,
    last_updated TEXT NOT NULL,
    PRIMARY KEY (politician_id, issuer_ticker),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ingest_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fec_mappings (
    politician_id TEXT NOT NULL,
    fec_candidate_id TEXT NOT NULL,
    bioguide_id TEXT NOT NULL,
    election_cycle INTEGER,
    last_synced TEXT NOT NULL,
    PRIMARY KEY (politician_id, fec_candidate_id),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_trades_politician ON trades(politician_id);
CREATE INDEX IF NOT EXISTS idx_trades_issuer ON trades(issuer_id);
CREATE INDEX IF NOT EXISTS idx_trades_pub_date ON trades(pub_date);
CREATE INDEX IF NOT EXISTS idx_trades_tx_date ON trades(tx_date);
CREATE INDEX IF NOT EXISTS idx_politicians_party ON politicians(party);
CREATE INDEX IF NOT EXISTS idx_politicians_state ON politicians(state_id);
CREATE INDEX IF NOT EXISTS idx_issuers_sector ON issuers(sector);
CREATE INDEX IF NOT EXISTS idx_trade_labels_label ON trade_labels(label);
CREATE INDEX IF NOT EXISTS idx_trade_committees_committee ON trade_committees(committee);
CREATE INDEX IF NOT EXISTS idx_politician_committees_committee ON politician_committees(committee);
CREATE INDEX IF NOT EXISTS idx_eod_prices_date ON issuer_eod_prices(price_date);
CREATE INDEX IF NOT EXISTS idx_trades_enriched ON trades(enriched_at);
CREATE INDEX IF NOT EXISTS idx_politicians_enriched ON politicians(enriched_at);
CREATE INDEX IF NOT EXISTS idx_issuers_enriched ON issuers(enriched_at);
CREATE INDEX IF NOT EXISTS idx_trades_price_enriched ON trades(price_enriched_at);
CREATE INDEX IF NOT EXISTS idx_positions_politician ON positions(politician_id);
CREATE INDEX IF NOT EXISTS idx_positions_ticker ON positions(issuer_ticker);
CREATE INDEX IF NOT EXISTS idx_positions_updated ON positions(last_updated);
CREATE INDEX IF NOT EXISTS idx_fec_mappings_fec_id ON fec_mappings(fec_candidate_id);
CREATE INDEX IF NOT EXISTS idx_fec_mappings_bioguide ON fec_mappings(bioguide_id);
