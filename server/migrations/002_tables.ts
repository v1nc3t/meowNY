import type { MigrationBuilder } from 'node-pg-migrate';

// Auth tables match better-auth 1.7.7 (uuid ids, snake_case). varchar limits are ours.

export function up(pgm: MigrationBuilder): void {
  const uuid = pgm.func('gen_random_uuid()');
  const now = pgm.func('now()');

  pgm.createTable('users', {
    id: { type: 'uuid', primaryKey: true, default: uuid },
    name: { type: 'varchar(100)', notNull: true },
    email: { type: 'varchar(254)', notNull: true },
    email_verified: { type: 'boolean', notNull: true, default: false },
    image: { type: 'text' },
    deletion_requested_at: { type: 'timestamptz' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('users', 'email', { name: 'users_email_uidx', unique: true });
  // Expression index stays SQL. createIndex parenthesizes function calls.
  pgm.sql('CREATE UNIQUE INDEX uq_users_email_ci ON users (lower(email));');

  pgm.createTable('sessions', {
    id: { type: 'uuid', primaryKey: true, default: uuid },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    token: { type: 'text', notNull: true },
    expires_at: { type: 'timestamptz', notNull: true },
    ip_address: { type: 'text' },
    user_agent: { type: 'text' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('sessions', 'token', { name: 'sessions_token_uidx', unique: true });
  pgm.createIndex('sessions', 'user_id', { name: 'sessions_user_id_idx' });
  pgm.createIndex('sessions', 'expires_at', { name: 'idx_sessions_expiry' });

  pgm.createTable('accounts', {
    id: { type: 'uuid', primaryKey: true, default: uuid },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    provider_id: { type: 'varchar(50)', notNull: true },
    account_id: { type: 'varchar(255)', notNull: true },
    password: { type: 'text' },
    access_token: { type: 'text' },
    refresh_token: { type: 'text' },
    id_token: { type: 'text' },
    access_token_expires_at: { type: 'timestamptz' },
    refresh_token_expires_at: { type: 'timestamptz' },
    scope: { type: 'text' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('accounts', ['provider_id', 'account_id'], {
    name: 'uq_accounts_provider_subject',
    unique: true,
  });
  pgm.createIndex('accounts', 'user_id', { name: 'accounts_user_id_idx' });

  pgm.createTable('verifications', {
    id: { type: 'uuid', primaryKey: true, default: uuid },
    identifier: { type: 'text', notNull: true },
    value: { type: 'text', notNull: true },
    expires_at: { type: 'timestamptz', notNull: true },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('verifications', 'identifier', { name: 'verifications_identifier_idx' });
  pgm.createIndex('verifications', 'expires_at', { name: 'idx_verifications_expiry' });

  pgm.createTable('user_settings', {
    user_id: {
      type: 'uuid',
      primaryKey: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    currency: { type: 'char(3)', notNull: true },
    locale: { type: 'varchar(35)', notNull: true, default: 'en' },
    timezone: { type: 'varchar(64)', notNull: true, default: 'UTC' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });

  pgm.createTable('user_consents', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    purpose: { type: 'varchar(40)', notNull: true },
    policy_version: { type: 'varchar(20)', notNull: true },
    granted_at: { type: 'timestamptz', notNull: true, default: now },
    withdrawn_at: { type: 'timestamptz' },
  });
  pgm.createIndex('user_consents', ['user_id', 'purpose', { name: 'granted_at', sort: 'DESC' }], {
    name: 'idx_consents_user_purpose',
  });

  pgm.createTable('privacy_requests', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: { type: 'uuid', notNull: true },
    type: { type: 'varchar(20)', notNull: true },
    status: { type: 'varchar(20)', notNull: true, default: 'PENDING' },
    requested_at: { type: 'timestamptz', notNull: true, default: now },
    completed_at: { type: 'timestamptz' },
    note: { type: 'varchar(255)' },
  });
  pgm.createIndex(
    'privacy_requests',
    ['user_id', { name: 'requested_at', sort: 'DESC' }],
    { name: 'idx_privacy_requests_user' },
  );

  pgm.createTable('category_groups', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    name: { type: 'varchar(50)', notNull: true },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('category_groups', ['user_id', 'name'], {
    name: 'uq_user_group_name',
    unique: true,
  });

  pgm.createTable('categories', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    category_group_id: {
      type: 'bigint',
      references: 'category_groups (id)',
      onDelete: 'SET NULL',
    },
    type: { type: 'varchar(10)', notNull: true },
    name: { type: 'varchar(50)', notNull: true },
    deleted_at: { type: 'timestamptz' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('categories', ['id', 'user_id'], {
    name: 'uq_categories_id_user',
    unique: true,
  });
  pgm.createIndex('categories', ['id', 'user_id', 'type'], {
    name: 'uq_categories_id_user_type',
    unique: true,
  });
  pgm.createIndex('categories', ['user_id', 'deleted_at'], { name: 'idx_categories_user_active' });
  pgm.createIndex('categories', 'category_group_id', { name: 'idx_categories_group' });

  pgm.createTable('budgets', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    type: { type: 'varchar(10)', notNull: true },
    scope: { type: 'varchar(10)', notNull: true },
    category_id: { type: 'bigint' },
    amount_limit: { type: 'numeric(12, 2)' },
    effective_from: { type: 'date', notNull: true },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('budgets', ['user_id', 'category_id', 'effective_from'], {
    name: 'uq_budget_category_month',
    unique: true,
  });
  pgm.createIndex('budgets', ['user_id', 'type', 'scope', 'category_id', 'effective_from'], {
    name: 'idx_budget_lookup',
  });
  pgm.createIndex('budgets', 'category_id', { name: 'idx_budgets_category' });
  pgm.addConstraint('budgets', 'fk_budgets_category_type', {
    foreignKeys: {
      columns: ['category_id', 'user_id', 'type'],
      references: 'categories (id, user_id, type)',
      onDelete: 'CASCADE',
    },
  });

  pgm.createTable('recurring_transactions', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    category_id: { type: 'bigint', notNull: true },
    name: { type: 'varchar(50)', notNull: true },
    description: { type: 'varchar(255)' },
    amount: { type: 'numeric(12, 2)', notNull: true },
    start_date: { type: 'date', notNull: true },
    interval_unit: { type: 'varchar(10)', notNull: true },
    interval_count: { type: 'smallint', notNull: true, default: 1 },
    next_due_date: { type: 'date', notNull: true },
    end_date: { type: 'date' },
    is_active: { type: 'boolean', notNull: true, default: true },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('recurring_transactions', ['is_active', 'next_due_date'], {
    name: 'idx_recurring_due',
  });
  pgm.createIndex('recurring_transactions', ['user_id', 'is_active', 'next_due_date'], {
    name: 'idx_recurring_user_due',
  });
  pgm.createIndex('recurring_transactions', 'category_id', { name: 'idx_recurring_category' });
  pgm.addConstraint('recurring_transactions', 'fk_recurring_category', {
    foreignKeys: {
      columns: ['category_id', 'user_id'],
      references: 'categories (id, user_id)',
      onDelete: 'NO ACTION',
    },
  });

  pgm.createTable('transactions', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: {
      type: 'uuid',
      notNull: true,
      references: 'users (id)',
      onDelete: 'CASCADE',
    },
    category_id: { type: 'bigint', notNull: true },
    recurring_transaction_id: {
      type: 'bigint',
      references: 'recurring_transactions (id)',
      onDelete: 'SET NULL',
    },
    recurring_due_date: { type: 'date' },
    client_uuid: { type: 'uuid' },
    name: { type: 'varchar(50)', notNull: true },
    amount: { type: 'numeric(12, 2)', notNull: true },
    planned_amount: { type: 'numeric(12, 2)' },
    payment_date: { type: 'date', notNull: true },
    description: { type: 'varchar(255)' },
    created_at: { type: 'timestamptz', notNull: true, default: now },
    updated_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('transactions', ['user_id', { name: 'payment_date', sort: 'DESC' }], {
    name: 'idx_tx_user_date',
  });
  pgm.createIndex(
    'transactions',
    ['user_id', 'category_id', { name: 'payment_date', sort: 'DESC' }],
    { name: 'idx_tx_user_category_date' },
  );
  pgm.createIndex('transactions', 'category_id', { name: 'idx_tx_category' });
  pgm.createIndex('transactions', ['recurring_transaction_id', 'recurring_due_date'], {
    name: 'uq_tx_recurring_occurrence',
    unique: true,
  });
  pgm.createIndex('transactions', ['user_id', 'client_uuid'], {
    name: 'uq_tx_client_uuid',
    unique: true,
  });
  pgm.addConstraint('transactions', 'fk_tx_category', {
    foreignKeys: {
      columns: ['category_id', 'user_id'],
      references: 'categories (id, user_id)',
      onDelete: 'NO ACTION',
    },
  });

  pgm.createTable('audit_log', {
    id: {
      type: 'bigint',
      primaryKey: true,
      sequenceGenerated: { precedence: 'BY DEFAULT' },
    },
    user_id: { type: 'uuid' },
    entity_type: { type: 'varchar(50)', notNull: true },
    entity_id: { type: 'varchar(64)', notNull: true },
    action: { type: 'varchar(10)', notNull: true },
    old_values: { type: 'jsonb' },
    new_values: { type: 'jsonb' },
    changed_at: { type: 'timestamptz', notNull: true, default: now },
  });
  pgm.createIndex('audit_log', ['user_id', { name: 'changed_at', sort: 'DESC' }], {
    name: 'idx_audit_user_time',
  });
  pgm.createIndex(
    'audit_log',
    ['entity_type', 'entity_id', { name: 'changed_at', sort: 'DESC' }],
    { name: 'idx_audit_entity' },
  );
}

export function down(pgm: MigrationBuilder): void {
  for (const table of [
    'audit_log',
    'transactions',
    'recurring_transactions',
    'budgets',
    'categories',
    'category_groups',
    'privacy_requests',
    'user_consents',
    'user_settings',
    'verifications',
    'accounts',
    'sessions',
    'users',
  ]) {
    pgm.dropTable(table, { ifExists: true, cascade: true });
  }
}
