import type { SharePerson } from "bmo-ui/app/domain/share-members";

// The people directory a console offers a bundle's screen (bmo-ui's `docs/console-bundles.md`, "The people
// directory"): a search of the reader's company, one account read by its key, and the reader's own. A
// console that offers none leaves the screen sharing by link alone.

export const CONSOLE_PEOPLE = "bmo-console-people";

export interface ConsolePeople {
  search: (words: string) => Promise<SharePerson[]>;
  person: (user: string) => Promise<SharePerson | null>;
  me: () => Promise<SharePerson | null>;
}

// The person an account key names, or, where the directory answers none for it, the key standing as the name.
export async function personOf(people: ConsolePeople, user: string): Promise<SharePerson> {
  const found = await people.person(user).catch(() => null);

  return found ?? { user, name: user, organization: "", jobTitle: "" };
}
