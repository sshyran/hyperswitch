-- This file should undo anything in `up.sql`
CREATE UNIQUE INDEX "merchant_connector_account_profile_id_connector_id_index" ON merchant_connector_account (profile_id, connector_name);

ALTER TABLE merchant_connector_account DROP CONSTRAINT "merchant_connector_account_profile_id_connector_label_key";
