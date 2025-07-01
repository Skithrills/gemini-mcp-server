export type InsertModelArgs = {
	query: string,
}

export type RunCodeArgs = {
	command: string,
}

export type ToolArgs = { InsertModel: InsertModelArgs } | { RunCode: RunCodeArgs }

export type ToolFunction = (ToolArgs) -> string?

return {}
