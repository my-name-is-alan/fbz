-- 0105_user_parental_controls.sql（additive）
-- Emby/JF UserPolicy.MaxParentalRating / BlockUnratedItems 最小闭环：
-- 存用户侧限制，Items 浏览按 official_rating 映射等级与未分级类型过滤。

alter table users
    add column if not exists max_parental_rating integer;

alter table users
    add column if not exists block_unrated_items text[] not null default '{}';

comment on column users.max_parental_rating is
    'Emby/JF Policy.MaxParentalRating; null = unrestricted; Items hide higher mapped official ratings';

comment on column users.block_unrated_items is
    'Emby/JF Policy.BlockUnratedItems (Movie/Series/…); Items hide unrated rows of those types';
