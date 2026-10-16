-- Books / AudioBook library type (Jellyfin `books` collection type).
alter table libraries drop constraint if exists libraries_library_type_check;
alter table libraries add constraint libraries_library_type_check
    check (library_type in ('movies', 'tvshows', 'music', 'homevideos', 'mixed', 'livetv', 'books'));

alter table media_items drop constraint if exists media_items_item_type_check;
alter table media_items add constraint media_items_item_type_check
    check (item_type in ('folder', 'movie', 'series', 'season', 'episode', 'artist', 'album', 'track', 'collection', 'photo', 'video', 'tvchannel', 'program', 'recording', 'book', 'audiobook'));
