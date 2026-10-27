// From React Native Reusables (uniwind registry), trimmed to celmic's single dark theme.
import * as SwitchPrimitives from "@rn-primitives/switch";

import { cn } from "@/lib/utils";

function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitives.Root>) {
  return (
    <SwitchPrimitives.Root
      className={cn(
        "h-7 w-12 shrink-0 flex-row items-center rounded-full border border-transparent px-0.5",
        props.checked ? "bg-primary" : "bg-input",
        props.disabled && "opacity-50",
        className,
      )}
      {...props}
    >
      <SwitchPrimitives.Thumb
        className={cn(
          "size-6 rounded-full",
          props.checked ? "translate-x-5 bg-primary-foreground" : "translate-x-0 bg-foreground",
        )}
      />
    </SwitchPrimitives.Root>
  );
}

export { Switch };
