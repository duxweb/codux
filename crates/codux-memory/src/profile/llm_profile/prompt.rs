pub(in crate::profile) fn project_profile_system_prompt() -> &'static str {
    "You improve a repository-derived project profile for an AI coding memory system.\nReturn JSON only, no markdown fences, no commentary.\nUse only the provided deterministic profile. Do not invent dependencies, commands, settings, files, or product claims."
}

pub(in crate::profile) fn make_project_profile_llm_prompt(
    deterministic_profile: &str,
) -> String {
    format!(
        "Rewrite this deterministic repository profile into a concise, useful project overview for future AI coding sessions.\n\nReturn minified JSON only:\n{{\"content\":\"Project: ...\\nOverview: ...\\n\\nTech stack:\\n- ...\\n\\nCommon commands:\\n- ...\\n\\nTop-level directories:\\n- ...\\n\\nDetected modules:\\n- ...\"}}\n\nRules:\n- Preserve the section names: Project, Overview, Tech stack, Common commands, Top-level directories, Detected modules.\n- Use Source signals only as repository evidence to improve Overview and Detected modules; do not copy long source snippets into the final profile.\n- Use Project memory signals only as durable post-scan corrections or additions; let them refine purpose, architecture, modules, tech stack, or commands when they are more specific than repository metadata.\n- Use only the provided deterministic profile; do not invent missing facts.\n- Keep it compact, target 500-900 tokens total.\n- Prefer concrete engineering facts over marketing language.\n- Merge duplicates and improve wording naturally; do not hard-truncate or cut mid-sentence.\n- Keep commands exactly as shown unless correcting obvious package-manager wording from the evidence.\n- If evidence is missing, omit that bullet instead of guessing.\n\nDeterministic profile:\n<profile>\n{}\n</profile>",
        deterministic_profile
    )
}
