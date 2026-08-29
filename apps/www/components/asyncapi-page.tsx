'use client';

import { createAsyncAPIPage } from '@fumadocs/asyncapi/ui';
import { Children, cloneElement, isValidElement, type ReactNode } from 'react';

type AsyncAPIElementProps = {
  children?: ReactNode;
  defaultValue?: string | string[];
  type?: string;
  value?: string;
};

function expandMessageAccordion(messages: ReactNode): ReactNode {
  if (!isValidElement<AsyncAPIElementProps>(messages)) return messages;

  const children = Children.toArray(messages.props.children);
  const expandedChildren = children.map((child) => {
    if (!isValidElement<AsyncAPIElementProps>(child) || child.props.type !== 'multiple') {
      return child;
    }

    const values = Children.toArray(child.props.children).flatMap((item) => {
      if (!isValidElement<AsyncAPIElementProps>(item) || !item.props.value) return [];
      return [item.props.value];
    });

    return values.length > 0 ? cloneElement(child, { defaultValue: values }) : child;
  });

  return cloneElement(messages, { children: expandedChildren });
}

/** Client renderer for generated Neplex OpenPrinter Cloud gateway pages. */
export const AsyncAPIPage = createAsyncAPIPage({
  content: {
    renderOperationLayout: (slots) => (
      <div className="@container flex flex-col gap-6 text-sm">
        {slots.header}
        {slots.description}
        {slots.server}
        {slots.channel}
        {slots.authSchemes}
        {slots.parameters}
        {expandMessageAccordion(slots.messages)}
        {slots.reply}
        {slots.bindings}
      </div>
    ),
  },
  schemaUI: {
    showExample: true,
  },
  storageKeyPrefix: 'openprinter-agent-gateway-',
});
