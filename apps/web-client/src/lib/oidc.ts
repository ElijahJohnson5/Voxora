const CLIENT_ID = "voxora-web";
const REDIRECT_URI = `${window.location.origin}/callback`;
const SCOPES = "openid profile email pods";
const HUB_URL = import.meta.env.VITE_HUB_URL || "http://localhost:4001";

// --- PKCE helpers ---

function base64url(input: Uint8Array | ArrayBuffer): string {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export function generateCodeVerifier(): string {
  const buffer = new Uint8Array(64);
  crypto.getRandomValues(buffer);
  return base64url(buffer);
}

export async function computeCodeChallenge(verifier: string): Promise<string> {
  const encoded = new TextEncoder().encode(verifier);
  const digest = await crypto.subtle.digest("SHA-256", encoded);
  return base64url(digest);
}

export function generateState(): string {
  const buffer = new Uint8Array(32);
  crypto.getRandomValues(buffer);
  return base64url(buffer);
}

// --- Token endpoint (form-urlencoded per OAuth 2.0 spec) ---

interface TokenResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
  refresh_token?: string;
  id_token?: string;
  scope: string;
}

async function postToken(body: Record<string, string>): Promise<TokenResponse> {
  const res = await fetch(`${HUB_URL}/oidc/token`, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(body),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`Token request failed (${res.status}): ${text}`);
  }

  return res.json();
}

// --- Login flow ---

export async function startLogin(): Promise<void> {
  const verifier = generateCodeVerifier();
  const challenge = await computeCodeChallenge(verifier);
  const state = generateState();

  sessionStorage.setItem("oidc_code_verifier", verifier);
  sessionStorage.setItem("oidc_state", state);

  // Resolve effective theme to pass to the Hub login page
  const storedTheme = localStorage.getItem("voxora-theme") || "system";
  const resolvedTheme =
    storedTheme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : storedTheme;

  const params = new URLSearchParams({
    response_type: "code",
    client_id: CLIENT_ID,
    redirect_uri: REDIRECT_URI,
    code_challenge: challenge,
    code_challenge_method: "S256",
    state,
    scope: SCOPES,
    theme: resolvedTheme,
  });

  window.location.href = `${HUB_URL}/oidc/authorize?${params.toString()}`;
}

// --- Callback handler ---

export interface TokenResult {
  accessToken: string;
  refreshToken: string | null;
  idToken: string | null;
  expiresIn: number;
}

export async function handleCallback(
  code: string,
  state: string,
): Promise<TokenResult> {
  const savedState = sessionStorage.getItem("oidc_state");
  const verifier = sessionStorage.getItem("oidc_code_verifier");

  if (!savedState || state !== savedState) {
    throw new Error("Invalid OAuth state — possible CSRF attack");
  }
  if (!verifier) {
    throw new Error("Missing PKCE code verifier");
  }

  sessionStorage.removeItem("oidc_state");
  sessionStorage.removeItem("oidc_code_verifier");

  const data = await postToken({
    grant_type: "authorization_code",
    code,
    code_verifier: verifier,
    redirect_uri: REDIRECT_URI,
    client_id: CLIENT_ID,
  });

  return {
    accessToken: data.access_token,
    refreshToken: data.refresh_token ?? null,
    idToken: data.id_token ?? null,
    expiresIn: data.expires_in,
  };
}

// --- Token refresh ---
export async function refreshAccessToken(
  refreshToken: string,
): Promise<TokenResult> {
  const data = await postToken({
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    client_id: CLIENT_ID,
  });

  return {
    accessToken: data.access_token,
    refreshToken: data.refresh_token ?? null,
    idToken: data.id_token ?? null,
    expiresIn: data.expires_in,
  };
}
