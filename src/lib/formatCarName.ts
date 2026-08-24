/**
 * Turn raw car ids / slugs into a readable label.
 * e.g. `ford_mustang_gt3` → `Ford Mustang GT3`
 */
export function formatCarName(car: string): string {
  const trimmed = car.trim();
  if (!trimmed) return car;

  return trimmed
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((token) => {
      // Tokens with digits (gt3, m4, 911) stay fully uppercase.
      if (/\d/.test(token)) {
        return token.toUpperCase();
      }
      return token.charAt(0).toUpperCase() + token.slice(1).toLowerCase();
    })
    .join(" ");
}
