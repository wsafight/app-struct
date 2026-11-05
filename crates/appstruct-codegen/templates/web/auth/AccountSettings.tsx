import { useState, type FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { QRCodeSVG } from "qrcode.react";
import { productApi, type ProductProfile } from "../generated/client";
import { errorMessage, triggerDownload } from "../resource";
import { ActionFeedback, LoadingState, t, useActionFeedback } from "../ui";
import { appQueryKeys } from "../query";
import { useUnsavedChanges } from "../navigation";
import { ConfirmDialog } from "../components/Dialog";

export function AccountSettings() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: appQueryKeys.product.profile,
    queryFn: ({ signal }) => productApi.profile({ signal }),
  });
  const profileAction = useActionFeedback(errorMessage);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState(false);
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [email, setEmail] = useState("");
  const [setup, setSetup] = useState<{ secret: string; uri: string } | null>(
    null,
  );
  const [recovery, setRecovery] = useState<string[]>([]);
  const [savedRecovery, setSavedRecovery] = useState(false);
  const blocker = useUnsavedChanges(recovery.length > 0 && !savedRecovery);
  const proof = { password, code: code || undefined };
  const emailToken = new URLSearchParams(window.location.search).get(
    "email_token",
  );
  async function action(operation: () => Promise<void>, refresh = true) {
    if (pending) return;
    setPending(true);
    setError("");
    setNotice("");
    try {
      await operation();
      if (refresh)
        await client.invalidateQueries({
          queryKey: appQueryKeys.product.profile,
        });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
      setPassword("");
      setCode("");
    }
  }
  async function copy(value: string, message: string) {
    setError("");
    setNotice("");
    try {
      if (!navigator.clipboard?.writeText)
        throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(value);
      setNotice(message);
    } catch {
      setError(
        t("Clipboard unavailable. Select and copy the visible text instead."),
      );
    }
  }
  function signIn() {
    window.location.assign("/login?next=%2Faccount");
  }
  const codeField = (
    <label>
      {t("Authenticator or recovery code")}
      <input
        autoComplete="one-time-code"
        inputMode={setup ? "numeric" : "text"}
        value={code}
        disabled={pending}
        onChange={(event) => setCode(event.target.value)}
      />
    </label>
  );
  return (
    <div className="account-settings">
      <ActionFeedback
        error={
          error ||
          profileAction.error ||
          (query.error ? errorMessage(query.error) : "")
        }
        notice={notice || profileAction.notice}
        onRetry={query.error ? () => void query.refetch() : undefined}
      />
      {query.isPending && <LoadingState />}
      {query.data && (
        <ProfileForm
          profile={query.data}
          pending={profileAction.isPending("profile")}
          save={(input) => {
            setNotice("");
            return profileAction.run(
              "profile",
              async () => {
                await productApi.updateProfile(input);
                await client.invalidateQueries({
                  queryKey: appQueryKeys.product.profile,
                });
              },
              t("Settings saved"),
            );
          }}
        />
      )}
      {emailToken && (
        <section className="panel">
          <h2>{t("Confirm new email address")}</h2>
          <button
            type="button"
            disabled={pending}
            onClick={() =>
              void action(async () => {
                await productApi.confirmEmail(emailToken);
                signIn();
              }, false)
            }
          >
            {t("Confirm email and sign in again")}
          </button>
        </section>
      )}
      {query.data && (
        <section className="panel settings-form">
          <h2>{t("Account security")}</h2>
          <p>
            {t(
              "Security changes require your current password and an authenticator or recovery code when MFA is enabled.",
            )}
          </p>
          <label>
            {t("Current password")}
            <input
              type="password"
              autoComplete="current-password"
              value={password}
              disabled={pending}
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
          {!setup && codeField}
          <details className="settings-section">
            <summary>{t("Change password")}</summary>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void action(async () => {
                  await productApi.changePassword(proof, newPassword);
                  setNewPassword("");
                  signIn();
                }, false);
              }}
            >
              <label>
                {t("New password")}
                <input
                  type="password"
                  autoComplete="new-password"
                  minLength={12}
                  required
                  value={newPassword}
                  disabled={pending}
                  onChange={(event) => setNewPassword(event.target.value)}
                />
              </label>
              <button disabled={pending || !password}>
                {t("Change password and revoke sessions")}
              </button>
            </form>
          </details>
          <details className="settings-section">
            <summary>{t("Change email")}</summary>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void action(async () => {
                  await productApi.changeEmail(proof, email);
                  setNotice(t("Check your new email for a confirmation link."));
                });
              }}
            >
              <label>
                {t("New email")}
                <input
                  type="email"
                  required
                  value={email}
                  disabled={pending}
                  onChange={(event) => setEmail(event.target.value)}
                />
              </label>
              <button disabled={pending || !password}>
                {t("Request email change")}
              </button>
            </form>
          </details>
          <details className="settings-section danger-zone">
            <summary>{t("Sessions and API tokens")}</summary>
            <p>
              {t(
                "This signs you out on every device and revokes existing API tokens.",
              )}
            </p>
            <button
              type="button"
              disabled={pending || !password}
              onClick={() =>
                void action(async () => {
                  await productApi.revokeSessions(proof);
                  signIn();
                }, false)
              }
            >
              {t("Revoke all sessions and API tokens")}
            </button>
          </details>
          <section className="mfa-section" aria-labelledby="mfa-title">
            <h3 id="mfa-title">{t("Two-factor authentication")}</h3>
            <p>{query.data.mfa_enabled ? t("Enabled") : t("Not enabled")}</p>
            {!query.data.mfa_enabled && !setup && (
              <button
                type="button"
                disabled={pending || !password}
                onClick={() =>
                  void action(async () =>
                    setSetup(await productApi.setupMfa(proof)),
                  )
                }
              >
                {t("Set up authenticator")}
              </button>
            )}
            {setup && (
              <div className="mfa-setup">
                <ol
                  className="mfa-steps"
                  aria-label={t("Authenticator setup steps")}
                >
                  <li>{t("Scan the QR code")}</li>
                  <li aria-current="step">{t("Verify a code")}</li>
                  <li>{t("Save recovery codes")}</li>
                </ol>
                <p>
                  {t(
                    "Scan this code with your authenticator app. The QR code is generated on this device.",
                  )}
                </p>
                <QRCodeSVG
                  value={setup.uri}
                  size={192}
                  marginSize={4}
                  level="M"
                  role="img"
                  title={t("Authenticator setup QR code")}
                  aria-label={t("Authenticator setup QR code")}
                />
                <details>
                  <summary>{t("Enter a setup key instead")}</summary>
                  <code className="mfa-secret">{setup.secret}</code>
                  <div className="row-actions">
                    <button
                      type="button"
                      onClick={() =>
                        void copy(setup.secret, t("Setup key copied"))
                      }
                    >
                      {t("Copy setup key")}
                    </button>
                    <a href={setup.uri}>{t("Open authenticator")}</a>
                  </div>
                </details>
                {codeField}
                <div className="row-actions">
                  <button
                    type="button"
                    disabled={pending || !/^\d{6}$/.test(code)}
                    onClick={() =>
                      void action(async () => {
                        const result = await productApi.enableMfa(code);
                        setRecovery(result.recovery_codes);
                        setSavedRecovery(false);
                        setSetup(null);
                      })
                    }
                  >
                    {t(pending ? "Working..." : "Enable MFA")}
                  </button>
                  <button
                    type="button"
                    disabled={pending}
                    onClick={() => {
                      setSetup(null);
                      setCode("");
                    }}
                  >
                    {t("Cancel setup")}
                  </button>
                </div>
              </div>
            )}
            {query.data.mfa_enabled && recovery.length === 0 && (
              <button
                type="button"
                disabled={pending || !password || !code}
                onClick={() =>
                  void action(async () => {
                    await productApi.disableMfa(proof);
                    signIn();
                  }, false)
                }
              >
                {t("Disable MFA and sign in again")}
              </button>
            )}
            {recovery.length > 0 && (
              <section
                className="recovery-codes"
                aria-labelledby="recovery-title"
              >
                <h3 id="recovery-title">{t("Save your recovery codes now")}</h3>
                <p role="status">
                  {t(
                    "Each code works once. These codes will not be shown again.",
                  )}
                </p>
                <ul>
                  {recovery.map((item) => (
                    <li key={item}>
                      <code>{item}</code>
                    </li>
                  ))}
                </ul>
                <div className="row-actions">
                  <button
                    type="button"
                    onClick={() =>
                      void copy(recovery.join("\n"), t("Recovery codes copied"))
                    }
                  >
                    {t("Copy recovery codes")}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      triggerDownload(
                        new Blob(
                          [
                            `${t("MFA recovery codes")}\n\n${recovery.join("\n")}\n`,
                          ],
                          { type: "text/plain;charset=utf-8" },
                        ),
                        "mfa-recovery-codes.txt",
                      );
                      setNotice(
                        t("Recovery codes downloaded. Store the file safely."),
                      );
                    }}
                  >
                    {t("Download recovery codes")}
                  </button>
                </div>
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={savedRecovery}
                    onChange={(event) => setSavedRecovery(event.target.checked)}
                  />
                  {t("I stored these codes in a safe place")}
                </label>
                <button
                  type="button"
                  disabled={!savedRecovery}
                  onClick={() => {
                    setRecovery([]);
                    setSavedRecovery(false);
                  }}
                >
                  {t("I saved my recovery codes")}
                </button>
              </section>
            )}
          </section>
        </section>
      )}
      <ConfirmDialog
        open={blocker.blocked}
        title={t("Leave without saving recovery codes?")}
        description={t(
          "These codes cannot be displayed again. Save them before leaving this page.",
        )}
        confirmLabel={t("Leave anyway")}
        danger
        onCancel={blocker.reset}
        onConfirm={blocker.proceed}
      />
    </div>
  );
}

function ProfileForm({
  profile,
  pending,
  save,
}: {
  profile: ProductProfile;
  pending: boolean;
  save: (input: Omit<ProductProfile, "mfa_enabled">) => Promise<unknown>;
}) {
  const [name, setName] = useState(profile.display_name);
  const [locale, setLocale] = useState(profile.locale);
  const [timezone, setTimezone] = useState(profile.timezone);
  const [notifications, setNotifications] = useState(profile.notifications);
  function submit(event: FormEvent) {
    event.preventDefault();
    void save({ display_name: name, locale, timezone, notifications });
  }
  return (
    <form className="panel settings-form" onSubmit={submit}>
      <h2>{t("Profile and preferences")}</h2>
      <label>
        {t("Display name")}
        <input
          value={name}
          maxLength={120}
          disabled={pending}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <label>
        {t("Report language")}
        <select
          value={locale}
          disabled={pending}
          onChange={(event) => setLocale(event.target.value)}
        >
          <option value="en-US">English</option>
          <option value="zh-CN">简体中文</option>
        </select>
      </label>
      <label>
        {t("Report timezone")}
        <select
          value={timezone}
          disabled={pending}
          onChange={(event) => setTimezone(event.target.value)}
        >
          <option value="UTC">UTC</option>
          <option value="Asia/Shanghai">Asia/Shanghai</option>
        </select>
      </label>
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={notifications}
          disabled={pending}
          onChange={(event) => setNotifications(event.target.checked)}
        />
        {t("Receive in-app notifications")}
      </label>
      <button disabled={pending}>
        {t(pending ? "Saving..." : "Save preferences")}
      </button>
    </form>
  );
}
