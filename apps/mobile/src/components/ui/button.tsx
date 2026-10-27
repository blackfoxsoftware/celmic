// From React Native Reusables (uniwind registry), trimmed to celmic's single dark theme.
import { cva, type VariantProps } from "class-variance-authority";
import { Pressable } from "react-native";

import { TextClassContext } from "@/components/ui/text";
import { cn } from "@/lib/utils";

const buttonVariants = cva("shrink-0 flex-row items-center justify-center gap-2 rounded-md", {
  variants: {
    variant: {
      default: "bg-primary active:bg-primary/85",
      outline: "border border-input bg-background active:bg-accent",
      secondary: "bg-secondary active:bg-secondary/80",
      ghost: "active:bg-accent",
    },
    size: {
      default: "h-12 px-5",
      sm: "h-10 px-4",
      lg: "h-14 px-6",
      icon: "h-12 w-12",
    },
  },
  defaultVariants: { variant: "default", size: "default" },
});

const buttonTextVariants = cva("font-sans-semibold text-base text-foreground", {
  variants: {
    variant: {
      default: "text-primary-foreground",
      outline: "",
      secondary: "text-secondary-foreground",
      ghost: "",
    },
    size: { default: "", sm: "text-sm", lg: "text-lg", icon: "" },
  },
  defaultVariants: { variant: "default", size: "default" },
});

type ButtonProps = React.ComponentProps<typeof Pressable> & VariantProps<typeof buttonVariants>;

function Button({ className, variant, size, ...props }: ButtonProps) {
  return (
    <TextClassContext.Provider value={buttonTextVariants({ variant, size })}>
      <Pressable
        className={cn(props.disabled && "opacity-50", buttonVariants({ variant, size }), className)}
        role="button"
        {...props}
      />
    </TextClassContext.Provider>
  );
}

export { Button, buttonTextVariants, buttonVariants };
export type { ButtonProps };
