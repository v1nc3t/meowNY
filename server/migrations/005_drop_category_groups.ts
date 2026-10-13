import type { MigrationBuilder } from 'node-pg-migrate';

// Removes category groups. up() deletes every group and each category's group link.
// down() restores the empty structure only. The deleted rows do not come back.

// Same function as 004, with or without the category_groups entry.
function exportUserData(withGroups: boolean): string {
  const groups = withGroups
    ? `    'category_groups', COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.id)
                                 FROM category_groups g WHERE g.user_id = p_user), '[]'::jsonb),
`
    : '';
  return `
CREATE OR REPLACE FUNCTION export_user_data(p_user uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'exported_at', now(),
    'profile', (SELECT to_jsonb(u) FROM users u WHERE u.id = p_user),
    'settings', (SELECT to_jsonb(s) FROM user_settings s WHERE s.user_id = p_user),
    'linked_logins', COALESCE((
        SELECT jsonb_agg(jsonb_build_object('provider', a.provider_id, 'linked_at', a.created_at))
        FROM accounts a WHERE a.user_id = p_user), '[]'::jsonb),
    'sessions', COALESCE((
        SELECT jsonb_agg(jsonb_build_object(
          'created_at', x.created_at,
          'expires_at', x.expires_at,
          'ip_address', x.ip_address,
          'user_agent', x.user_agent))
        FROM sessions x WHERE x.user_id = p_user), '[]'::jsonb),
    'consents', COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id)
                          FROM user_consents c WHERE c.user_id = p_user), '[]'::jsonb),
${groups}    'categories', COALESCE((SELECT jsonb_agg(to_jsonb(k) ORDER BY k.id)
                            FROM categories k WHERE k.user_id = p_user), '[]'::jsonb),
    'budgets', COALESCE((SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id)
                         FROM budgets b WHERE b.user_id = p_user), '[]'::jsonb),
    'recurring_transactions', COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id)
                                        FROM recurring_transactions r WHERE r.user_id = p_user), '[]'::jsonb),
    'transactions', COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id)
                              FROM transactions t WHERE t.user_id = p_user), '[]'::jsonb),
    'history', COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id)
                         FROM audit_log h WHERE h.user_id = p_user), '[]'::jsonb)
  )
$$;
`;
}

export function up(pgm: MigrationBuilder): void {
  // The index and the foreign key on the column go with it. The table's triggers go with the table.
  pgm.dropColumn('categories', 'category_group_id');
  pgm.dropTable('category_groups');
  pgm.sql("DELETE FROM audit_log WHERE entity_type = 'category_groups'");
  pgm.sql(exportUserData(false));
}

export function down(pgm: MigrationBuilder): void {
  const now = pgm.func('now()');

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

  pgm.addColumn('categories', {
    category_group_id: {
      type: 'bigint',
      references: 'category_groups (id)',
      onDelete: 'SET NULL',
    },
  });
  pgm.createIndex('categories', 'category_group_id', { name: 'idx_categories_group' });

  // Triggers stay SQL, as in 003.
  pgm.sql(`
CREATE TRIGGER trg_updated_at BEFORE UPDATE ON category_groups
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_audit AFTER INSERT OR UPDATE OR DELETE ON category_groups
  FOR EACH ROW EXECUTE FUNCTION audit_row_change();
`);
  pgm.sql(exportUserData(true));
}
