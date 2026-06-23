create or replace function fbz_cron_field_matches(
    field text,
    candidate integer,
    min_value integer,
    max_value integer,
    sunday_seven boolean default false
)
returns boolean
language plpgsql
immutable
strict
as $$
declare
    token text;
    base text;
    step_value integer;
    lower_value integer;
    upper_value integer;
    parts text[];
    range_parts text[];
    value integer;
    normalized_value integer;
begin
    if field is null or btrim(field) = '' then
        return false;
    end if;

    foreach token in array regexp_split_to_array(btrim(field), '\s*,\s*')
    loop
        token := btrim(token);
        if token = '' then
            return false;
        end if;

        parts := regexp_split_to_array(token, '/');
        if array_length(parts, 1) = 1 then
            base := parts[1];
            step_value := 1;
        elsif array_length(parts, 1) = 2 then
            base := parts[1];
            if parts[2] !~ '^\d+$' then
                return false;
            end if;
            step_value := parts[2]::integer;
            if step_value <= 0 then
                return false;
            end if;
        else
            return false;
        end if;

        if base = '*' then
            lower_value := min_value;
            upper_value := max_value;
        elsif base ~ '^\d+$' then
            lower_value := base::integer;
            upper_value := lower_value;
        elsif base ~ '^\d+-\d+$' then
            range_parts := regexp_split_to_array(base, '-');
            lower_value := range_parts[1]::integer;
            upper_value := range_parts[2]::integer;
        else
            return false;
        end if;

        if lower_value < min_value or upper_value > max_value or lower_value > upper_value then
            return false;
        end if;

        value := lower_value;
        while value <= upper_value loop
            normalized_value := value;
            if sunday_seven and value = 7 then
                normalized_value := 0;
            end if;
            if normalized_value = candidate then
                return true;
            end if;
            value := value + step_value;
        end loop;
    end loop;

    return false;
end;
$$;

create or replace function fbz_next_cron_run_at(
    expression text,
    from_time timestamptz
)
returns timestamptz
language plpgsql
stable
strict
as $$
declare
    fields text[];
    minute_field text;
    hour_field text;
    day_of_month_field text;
    month_field text;
    day_of_week_field text;
    day_of_month_restricted boolean;
    day_of_week_restricted boolean;
    next_run timestamptz;
begin
    fields := regexp_split_to_array(btrim(expression), '\s+');
    if array_length(fields, 1) <> 5 then
        return null;
    end if;

    minute_field := fields[1];
    hour_field := fields[2];
    day_of_month_field := fields[3];
    month_field := fields[4];
    day_of_week_field := fields[5];
    day_of_month_restricted := day_of_month_field <> '*';
    day_of_week_restricted := day_of_week_field <> '*';

    select candidate_at
    into next_run
    from generate_series(
        date_trunc('minute', from_time) + interval '1 minute',
        from_time + interval '366 days',
        interval '1 minute'
    ) as candidate_at
    where fbz_cron_field_matches(minute_field, date_part('minute', candidate_at)::integer, 0, 59, false)
      and fbz_cron_field_matches(hour_field, date_part('hour', candidate_at)::integer, 0, 23, false)
      and fbz_cron_field_matches(month_field, date_part('month', candidate_at)::integer, 1, 12, false)
      and (
          case
              when day_of_month_restricted and day_of_week_restricted then
                  fbz_cron_field_matches(day_of_month_field, date_part('day', candidate_at)::integer, 1, 31, false)
                  or fbz_cron_field_matches(day_of_week_field, date_part('dow', candidate_at)::integer, 0, 7, true)
              else
                  fbz_cron_field_matches(day_of_month_field, date_part('day', candidate_at)::integer, 1, 31, false)
                  and fbz_cron_field_matches(day_of_week_field, date_part('dow', candidate_at)::integer, 0, 7, true)
          end
      )
    order by candidate_at asc
    limit 1;

    return next_run;
end;
$$;
