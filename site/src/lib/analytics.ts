import posthog from 'posthog-js';

/** The PostHog project's public token, safe to ship: it can only send events. */
const key = import.meta.env.PUBLIC_POSTHOG_KEY || 'phc_CGvb9QB3s7PyZdAoMRC6ZyGMbTPqSNVD4XPC7dJaQwGT';
/** `/ingest` is rewritten to PostHog by vercel.json, so blockers that list PostHog's own hosts let it through. */
const host = import.meta.env.PUBLIC_POSTHOG_HOST || '/ingest';

/** Starts PostHog on production builds only, so `astro dev` sends nothing. */
export function startAnalytics() {
  if (!import.meta.env.PROD || !key) return;
  posthog.init(key, {
    api_host: host,
    ui_host: 'https://us.posthog.com',
    defaults: '2026-08-30',
    capture_pageview: 'history_change',
    capture_pageleave: true,
    autocapture: true,
    disable_session_recording: true,
    person_profiles: 'identified_only',
  });
  posthog.register({ product: 'localhost' });

  // Key events are declared in the markup (`data-track="download_clicked"`, `data-track-platform="linux"`),
  // so components only describe what a click means. The list is in docs/analytics.md.
  document.addEventListener(
    'click',
    (event) => {
      const element = (event.target as Element | null)?.closest<HTMLElement>('[data-track]');
      if (!element) return;
      const properties: Record<string, string> = {};
      for (const [name, value] of Object.entries(element.dataset)) {
        if (name.startsWith('track') && name !== 'track' && value) properties[name.slice(5).replace(/^./, (c) => c.toLowerCase())] = value;
      }
      // Most of these clicks leave the page, so the event goes out now rather than with the next batch.
      posthog.capture(element.dataset.track!, properties, { send_instantly: true, transport: 'sendBeacon' });
    },
    { capture: true },
  );
}
