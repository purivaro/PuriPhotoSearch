import type { ReactNode } from 'react';
import { DropdownMenu, Popover as RadixPopover, Tooltip as RadixTooltip } from 'radix-ui';
import { cn } from '@/lib/cn';

export function Tooltip({ label, children, side = 'top' }: { label: string; children: ReactNode; side?: 'top' | 'right' | 'bottom' | 'left' }) {
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          side={side}
          sideOffset={6}
          className="anim-fade z-[90] max-w-[280px] rounded-lg bg-ink px-2.5 py-1.5 text-[12.5px] leading-snug text-white shadow-raised"
        >
          {label}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}

export const TooltipProvider = RadixTooltip.Provider;

export const Popover = RadixPopover.Root;
export const PopoverTrigger = RadixPopover.Trigger;
export const PopoverAnchor = RadixPopover.Anchor;
export const PopoverClose = RadixPopover.Close;

export function PopoverContent({
  children,
  className,
  align = 'start',
  side = 'bottom',
  ...rest
}: RadixPopover.PopoverContentProps) {
  return (
    <RadixPopover.Portal>
      <RadixPopover.Content
        align={align}
        side={side}
        sideOffset={8}
        collisionPadding={12}
        className={cn('anim-pop z-[80] rounded-xl border border-line bg-surface shadow-pop outline-none', className)}
        {...rest}
      >
        {children}
      </RadixPopover.Content>
    </RadixPopover.Portal>
  );
}

export const Menu = DropdownMenu.Root;
export const MenuTrigger = DropdownMenu.Trigger;

export function MenuContent({ children, align = 'end' }: { children: ReactNode; align?: 'start' | 'end' }) {
  return (
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        align={align}
        sideOffset={6}
        collisionPadding={12}
        className="anim-pop z-[80] min-w-[210px] rounded-xl border border-line bg-surface p-1.5 shadow-pop outline-none"
      >
        {children}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  );
}

export function MenuItem({
  children,
  icon,
  tone = 'default',
  onSelect,
  disabled,
}: {
  children: ReactNode;
  icon?: ReactNode;
  tone?: 'default' | 'danger';
  onSelect: () => void;
  disabled?: boolean;
}) {
  return (
    <DropdownMenu.Item
      disabled={disabled}
      onSelect={onSelect}
      className={cn(
        'flex min-h-9 cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-sm outline-none data-[disabled]:cursor-not-allowed data-[disabled]:opacity-45',
        tone === 'danger' ? 'text-danger data-[highlighted]:bg-danger-soft' : 'text-ink data-[highlighted]:bg-sunken',
      )}
    >
      {icon && <span className="flex size-4 shrink-0 items-center justify-center [&>svg]:size-4">{icon}</span>}
      {children}
    </DropdownMenu.Item>
  );
}

export function MenuSeparator() {
  return <DropdownMenu.Separator className="my-1 h-px bg-line" />;
}
