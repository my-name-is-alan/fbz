-- Library-level image-language override (design metadata-scraper-design.md §7.2
-- "库级可选覆盖（后续）"). Lets a library pin its poster/backdrop language policy
-- independently of the text-metadata language, layering over the global defaults
-- from metadata_global_settings (id = 1). Purely additive: three nullable columns
-- on the existing libraries table, no changes to existing rows or constraints.
--
-- Precedence at lookup time (see metadata/service.rs build_lookup):
--   library override (these columns) ← global default ← provider built-in.

alter table libraries
    add column if not exists preferred_image_language text,
    add column if not exists preferred_image_prefer_original boolean,
    add column if not exists preferred_image_fallback_languages text[];

-- Locale shape guard, mirroring metadata_global_settings.image_language. NULL is
-- always allowed (means "inherit the global / provider default").
alter table libraries
    add constraint libraries_preferred_image_language_shape
    check (
        preferred_image_language is null
        or length(trim(preferred_image_language)) between 1 and 16
    );
