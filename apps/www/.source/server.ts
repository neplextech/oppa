// @ts-nocheck
import * as __fd_glob_21 from "../content/docs/why-oppa-exists.mdx?collection=docs"
import * as __fd_glob_20 from "../content/docs/virtual-printer.mdx?collection=docs"
import * as __fd_glob_19 from "../content/docs/server-sdk.mdx?collection=docs"
import * as __fd_glob_18 from "../content/docs/security.mdx?collection=docs"
import * as __fd_glob_17 from "../content/docs/release-process.mdx?collection=docs"
import * as __fd_glob_16 from "../content/docs/protocol.mdx?collection=docs"
import * as __fd_glob_15 from "../content/docs/product-configuration.mdx?collection=docs"
import * as __fd_glob_14 from "../content/docs/printer-discovery.mdx?collection=docs"
import * as __fd_glob_13 from "../content/docs/print-documents.mdx?collection=docs"
import * as __fd_glob_12 from "../content/docs/nodejs-integration.mdx?collection=docs"
import * as __fd_glob_11 from "../content/docs/job-delivery.mdx?collection=docs"
import * as __fd_glob_10 from "../content/docs/index.mdx?collection=docs"
import * as __fd_glob_9 from "../content/docs/idempotency.mdx?collection=docs"
import * as __fd_glob_8 from "../content/docs/getting-started.mdx?collection=docs"
import * as __fd_glob_7 from "../content/docs/example-server.mdx?collection=docs"
import * as __fd_glob_6 from "../content/docs/diagnostics.mdx?collection=docs"
import * as __fd_glob_5 from "../content/docs/contributing.mdx?collection=docs"
import * as __fd_glob_4 from "../content/docs/build-oppa.mdx?collection=docs"
import * as __fd_glob_3 from "../content/docs/authentication-flow.mdx?collection=docs"
import * as __fd_glob_2 from "../content/docs/architecture.mdx?collection=docs"
import * as __fd_glob_1 from "../content/docs/agent-lifecycle.mdx?collection=docs"
import { default as __fd_glob_0 } from "../content/docs/meta.json?collection=docs"
import { server } from 'fumadocs-mdx/runtime/server';
import type * as Config from '../source.config';

const create = server<typeof Config, import("fumadocs-mdx/runtime/types").InternalTypeConfig & {
  DocData: {
  }
}>();

export const docs = await create.docs("docs", "content/docs", {"meta.json": __fd_glob_0, }, {"agent-lifecycle.mdx": __fd_glob_1, "architecture.mdx": __fd_glob_2, "authentication-flow.mdx": __fd_glob_3, "build-oppa.mdx": __fd_glob_4, "contributing.mdx": __fd_glob_5, "diagnostics.mdx": __fd_glob_6, "example-server.mdx": __fd_glob_7, "getting-started.mdx": __fd_glob_8, "idempotency.mdx": __fd_glob_9, "index.mdx": __fd_glob_10, "job-delivery.mdx": __fd_glob_11, "nodejs-integration.mdx": __fd_glob_12, "print-documents.mdx": __fd_glob_13, "printer-discovery.mdx": __fd_glob_14, "product-configuration.mdx": __fd_glob_15, "protocol.mdx": __fd_glob_16, "release-process.mdx": __fd_glob_17, "security.mdx": __fd_glob_18, "server-sdk.mdx": __fd_glob_19, "virtual-printer.mdx": __fd_glob_20, "why-oppa-exists.mdx": __fd_glob_21, });