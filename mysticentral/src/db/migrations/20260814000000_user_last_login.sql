-- Add last_login_at to users table
-- Required by frontend User contract (last_login_at?: string) and login flow

ALTER TABLE users ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMP WITH TIME ZONE;
