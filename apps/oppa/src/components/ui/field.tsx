import { cva, type VariantProps } from 'class-variance-authority';
import * as React from 'react';

import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

const fieldVariants = cva('group/field flex w-full gap-2 data-[invalid=true]:text-destructive', {
  variants: {
    orientation: {
      vertical: 'flex-col',
      horizontal: 'items-center',
      responsive: 'flex-col sm:flex-row sm:items-center',
    },
  },
  defaultVariants: {
    orientation: 'vertical',
  },
});

function Field({
  className,
  orientation = 'vertical',
  ...props
}: React.ComponentProps<'div'> & VariantProps<typeof fieldVariants>) {
  return (
    <div
      role="group"
      data-slot="field"
      data-orientation={orientation}
      className={cn(fieldVariants({ orientation }), className)}
      {...props}
    />
  );
}

function FieldGroup({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="field-group" className={cn('grid gap-4', className)} {...props} />;
}

function FieldContent({ className, ...props }: React.ComponentProps<'div'>) {
  return <div data-slot="field-content" className={cn('flex flex-1 flex-col gap-1.5', className)} {...props} />;
}

function FieldLabel({ className, ...props }: React.ComponentProps<typeof Label>) {
  return <Label data-slot="field-label" className={cn('text-xs', className)} {...props} />;
}

function FieldDescription({ className, ...props }: React.ComponentProps<'p'>) {
  return (
    <p data-slot="field-description" className={cn('text-muted-foreground text-xs leading-4', className)} {...props} />
  );
}

function FieldError({ className, ...props }: React.ComponentProps<'p'>) {
  return <p data-slot="field-error" className={cn('text-destructive text-xs leading-4', className)} {...props} />;
}

export { Field, FieldContent, FieldDescription, FieldError, FieldGroup, FieldLabel };
