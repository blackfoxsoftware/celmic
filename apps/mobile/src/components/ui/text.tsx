// From React Native Reusables (uniwind registry), trimmed to celmic's single dark theme and
// per-weight font families (React Native does not synthesize weights for loaded fonts).
import { Slot } from "@rn-primitives/slot";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { Text as RNText, type Role } from "react-native";

import { cn } from "@/lib/utils";

const textVariants = cva("font-sans text-base text-foreground", {
  variants: {
    variant: {
      default: "",
      display: "font-display text-5xl leading-none tracking-tight",
      label: "font-display-semibold text-sm uppercase tracking-[0.14em] text-muted-foreground",
      large: "font-sans-semibold text-lg",
      muted: "text-sm text-muted-foreground",
      figure: "font-mono",
    },
  },
  defaultVariants: { variant: "default" },
});

type TextVariantProps = VariantProps<typeof textVariants>;
type TextVariant = NonNullable<TextVariantProps["variant"]>;

const ROLE: Partial<Record<TextVariant, Role>> = { display: "heading" };

const TextClassContext = React.createContext<string | undefined>(undefined);

function Text({
  className,
  asChild = false,
  variant = "default",
  ...props
}: React.ComponentProps<typeof RNText> & TextVariantProps & { asChild?: boolean }) {
  const textClass = React.useContext(TextClassContext);
  const Component = asChild ? Slot : RNText;
  return (
    <Component
      className={cn(textVariants({ variant }), textClass, className)}
      role={variant ? ROLE[variant] : undefined}
      {...props}
    />
  );
}

export { Text, TextClassContext };
