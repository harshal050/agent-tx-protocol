/**
 * Client-safe search ranking (no Node.js or Markdown dependencies).
 * Import via `@agenttx/content/search-rank` in client components.
 */

export interface SearchEntry {
  id: string;
  href: string;
  title: string;
  section: string;
  heading: string | null;
  text: string;
}

function occurrences(haystack: string, needle: string): number {
  let count = 0;
  let from = 0;
  for (;;) {
    const at = haystack.indexOf(needle, from);
    if (at === -1) return count;
    count++;
    from = at + needle.length;
  }
}

function startsWord(haystack: string, needle: string): boolean {
  const at = haystack.indexOf(needle);
  return at === 0 || (at > 0 && /[\s\-_/.(`'"]/.test(haystack[at - 1]!));
}

/**
 * Ranks entries for a query. Every token must match the title, heading,
 * section or text; titles and headings weigh most, word starts beat
 * mid-word matches.
 */
export function rankSearch<T extends SearchEntry>(entries: readonly T[], query: string, limit = 12): T[] {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return [];

  const scored: { entry: T; score: number }[] = [];
  for (const entry of entries) {
    const title = entry.title.toLowerCase();
    const heading = (entry.heading ?? "").toLowerCase();
    const section = entry.section.toLowerCase();
    const text = entry.text.toLowerCase();

    let score = 0;
    let matchedAll = true;
    for (const token of tokens) {
      let tokenScore = 0;
      if (title.includes(token)) tokenScore += startsWord(title, token) ? 12 : 6;
      if (heading.includes(token)) tokenScore += startsWord(heading, token) ? 8 : 4;
      if (section.includes(token)) tokenScore += 1;
      const hits = occurrences(text, token);
      if (hits > 0) tokenScore += 1 + Math.min(hits, 4) * 0.5;
      if (tokenScore === 0) {
        matchedAll = false;
        break;
      }
      score += tokenScore;
    }
    if (!matchedAll) continue;
    if (entry.heading === null) score += 0.5;
    scored.push({ entry, score });
  }

  return scored
    .sort((a, b) => b.score - a.score || a.entry.id.localeCompare(b.entry.id))
    .slice(0, limit)
    .map((s) => s.entry);
}
