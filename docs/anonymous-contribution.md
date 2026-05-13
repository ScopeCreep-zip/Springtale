# Anonymous contribution

You can contribute to Springtale without revealing your identity.
We accept it; the project is built for users whose threat model
includes "real names get me hurt", and we extend the same
consideration to contributors.

This page is the OPSEC guide for contributing pseudonymously or
anonymously.

## What we don't require

- No legal name on commits.
- No real-name PGP key.
- No CLA (we use DCO sign-off only).
- No verified email on GitHub.
- No traceable identity anywhere in the contribution chain.

Code is judged on the diff. The author field can be a throwaway
handle.

## Setting up a contribution identity

A clean contribution identity is one that doesn't link back to your
real one. Treat it like a new persona, not like "your GitHub account
but with a fake name".

### GitHub side

1. **New GitHub account.** Sign up over Tor with a fresh email
   address (Proton Mail, Tuta, or a self-hosted that doesn't ask
   for SMS verification). GitHub may demand SMS verification for
   accounts created over Tor — use a SMS-receive service if so,
   accept that this is a friction point.
2. **No real photo, no real bio.** Generic avatar, generic bio.
   Don't list your timezone if your timezone narrows you.
3. **Don't follow accounts that link to your real identity.** The
   social graph is a deanonymization vector.
4. **Don't reuse the username from another service.** Even partial
   reuse is searchable.

### Git side

```bash
# In the cloned Springtale repo:
git config user.name "your-pseudonym"
git config user.email "your-pseudonym@something-noreply"
```

GitHub will let you use the privacy email
`{userid}+{username}@users.noreply.github.com`. Use it.

```bash
git config user.email "12345678+your-pseudonym@users.noreply.github.com"
```

Verify on every commit:

```bash
git log --format='%an <%ae>' | head -1
```

### DCO sign-off

```bash
git commit -s -m "your commit message"
```

The `-s` flag adds `Signed-off-by: pseudonym <pseudonym@email>`.
That's the DCO. We don't require a real identity for this.

## OPSEC during the contribution

### Network

- Push and review over Tor or a VPN you trust. GitHub sees your IP
  on every interaction; if your real IP is sensitive, don't use
  it.
- Don't use a VPN provider that requires identity to sign up.
- Browser-side, use Tor Browser or a hardened Firefox profile
  dedicated to the pseudonym. Don't sign into the pseudonymous
  GitHub from the same browser you use for your real one. Profile
  fingerprinting links the two.

### Timing

- Don't commit at times that narrow your timezone. If your
  commits cluster on US Pacific working hours, that's narrowing.
  Use `commit --date` to scatter timestamps if it matters.

### Writing style

- The hardest one. Linguistic style is highly identifying:
  vocabulary, idiom, comma usage, paragraph length, em-dash habit.
- If you're paranoid: keep contributions short. Long PRs are
  more identifying than terse ones. Mimic the project's existing
  voice rather than your natural one.
- Don't include personal anecdotes in PR descriptions.

### Code style

- Run `cargo fmt` so your formatting matches the project. Personal
  formatting tics are mildly identifying.
- Don't introduce idioms not used elsewhere in the codebase.

### File system metadata

- `git` strips most metadata. The diff is what matters.
- Be aware that EXIF data in screenshots (PR attachments) leaks
  device + location. Strip with `exiftool -all=` before posting.
- Don't paste from a workspace that has identity-linked files
  open; `xclip` sometimes drops clipboard headers.

## What gets persisted

Once your contribution lands in `main`:

- The commit hash, author name, email, and timestamp are part of
  the public git history. Forever.
- Tag immutably points at the commit. Future history can't change
  the author of a past commit.
- GitHub's mirror is also part of the public record.

This means a moment of bad OPSEC is permanent. If you reveal real
identity on one comment in the PR thread, the link from
pseudonym-to-real is permanent in GitHub's history. Be careful with
edits — GitHub keeps edit history; an edit doesn't hide the
original.

## How we handle reports tied to identity

If someone tries to dox a contributor through an issue, PR, or
discussion:

- We follow the [Code of Conduct](../CODE_OF_CONDUCT.md). Doxxing is
  an immediate permanent ban.
- We don't share contributor identity information with anyone,
  period. We don't have most contributors' real identities; for
  those we know personally, we won't disclose without explicit
  consent.
- If we receive legal compulsion to disclose, we'll do whatever the
  law permits to push back. We will tell the affected contributor
  unless prohibited.

## Caveats

- **GitHub itself sees you.** GitHub knows your IP, your browser,
  your timing. If your threat model includes GitHub being
  subpoenaed, contributing via GitHub isn't fully anonymous.
- **The maintainer has triage burden.** Pseudonymous contributors
  can't easily be vouched for in code review. We treat the diff
  on its merits, but a long history of good contributions from a
  known pseudonym builds trust faster than a one-off from a
  brand-new account.
- **Some contribution paths require real identity.** Speaking at a
  conference about the project, applying for a security grant on
  behalf of the project, public press — these are coordinated
  with the maintainer. You can contribute pseudonymously without
  doing any of those.

## Asking for help anonymously

The [GitHub Discussions](https://github.com/ScopeCreep-zip/Springtale/discussions)
forum is the right place for "I'm trying to do X and stuck".
You can post pseudonymously. Other contributors will help based
on the question, not the asker.

For sensitive questions you don't want public: email
`k1104jackson@hotmail.com`. Replies go to whatever pseudonymous
address you use. If you want signed mail, our PGP key is on the
maintainer's GitHub profile.

## Resources

- [EFF: Surveillance Self-Defense — Pseudonymity](https://ssd.eff.org/)
- [Threat Library — Anarsec](https://www.anarsec.guide/)
- [Securedrop](https://securedrop.org/) — for journalists; not
  directly applicable to OSS contribution but the OPSEC model
  translates.

If you want guidance specific to your threat model that this page
doesn't cover, ask. We'll answer publicly or privately depending on
what the question is.
