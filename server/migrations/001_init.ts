import type { MigrationBuilder } from 'node-pg-migrate';

export function up(pgm: MigrationBuilder): void {
  pgm.createExtension('pgcrypto', { ifNotExists: true });
}

export function down(pgm: MigrationBuilder): void {
  pgm.dropExtension('pgcrypto', { ifExists: true });
}
