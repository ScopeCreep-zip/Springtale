-- Track activation errors per rule so broken rules are visible in the
-- dashboard. Both Home Assistant and n8n persist these per automation/workflow.
ALTER TABLE rules ADD COLUMN activation_error TEXT;
