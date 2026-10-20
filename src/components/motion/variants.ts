import type { Transition } from "framer-motion";

/** Every RAPIDS motion resolves inside 150–200ms with an ease-out curve. */
export const TRANSITION: Transition = {
  duration: 0.18,
  ease: [0.33, 1, 0.68, 1],
};
