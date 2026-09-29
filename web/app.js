import { JefflagDate, format } from "jefflag";
import confetti from "canvas-confetti";

// Every stable on HorseTinder is in the Bay Area.
const STABLE_ZONE = "America/Los_Angeles";

for (const el of document.querySelectorAll("[data-local-time]")) {
  const now = JefflagDate.now(STABLE_ZONE);
  el.textContent = `🕐 ${format(now, "hh:mm A")} in ${el.dataset.city}`;
}

for (const el of document.querySelectorAll("[data-matched-at]")) {
  const matchedAt = JefflagDate.fromEpoch(Date.parse(el.dataset.matchedAt.replace(" ", "T") + "Z"), STABLE_ZONE);
  const minutes = Math.floor(JefflagDate.now(STABLE_ZONE).diff(matchedAt, "minute"));
  el.textContent =
    minutes < 1 ? "Matched just now"
    : minutes < 60 ? `Matched ${minutes} min ago`
    : `Matched ${format(matchedAt, "MMM DD, hh:mm A")}`;
}

if (document.querySelector(".match-toast")) {
  confetti({ particleCount: 120, spread: 70, origin: { y: 0.3 } });
}
