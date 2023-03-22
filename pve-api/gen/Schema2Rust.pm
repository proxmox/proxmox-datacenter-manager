package Schema2Rust;

use strict;
use warnings;

use Carp qw(confess carp croak cluck);

use PVE::JSONSchema;

use Bool;

our $API = 1;

my %__derive_default = (
    Debug => 1,
    'serde::Deserialize' => 1,
    'serde::Serialize' => 1,
);
sub derive_default : prototype(@) {
    return {
        %__derive_default,
        map { $_ => 1 } @_
    };
}

my $all_types = {};
my $all_structs = {};
my $all_enums = {};
my $all_methods = {};

my $registered_formats = {};
my $registered_derives = {};
my $rename_enum_variant = {};

my $dedup_struct = {};
my $dedup_enum = {};
my $dedup_array_module = {};

our $__err_path = '';

my sub to_doc_comment : prototype($);
my sub handle_def : prototype($$$);
sub generate_struct : prototype($$$);

sub dump {
    use Data::Dumper;
    print(Dumper($all_types));
}

my sub print_derive : prototype($$) {
    my ($out, $derive) = @_;
    return if !$derive->%*;
    $derive = join(', ', sort keys $derive->%*);
    print {$out} "#[derive($derive)]\n";
}

my sub count_defined_values : prototype($) {
    my ($hash) = @_;
    return scalar(grep { defined($hash->{$_}) } keys $hash->%*);
}

my sub api_to_string : prototype($$$$);
# $derive_optional => For structs only, if an api entry contains *only* the 'optional' flag then
#     we can just leave out the schema completely.
sub api_to_string : prototype($$$$) {
    my ($indent, $out, $api, $derive_optional) = @_;

    return if !$API || !$api->%*;

    $derive_optional //= '';

    for my $key (sort keys $api->%*) {
        my $value = $api->{$key};
        next if !defined($value);

        if (!ref($value)) {
            print {$out} "${indent}$key: $value,\n";
        } elsif (my $func = eval { $value->can('TO_API_SCHEMA') }) {
            $value = $value->$func();
            print {$out} "${indent}$key: $value,\n";
        } elsif (ref($value) eq 'HASH') {
            my $next_derive_optional = undef;
            if ($derive_optional eq 'struct' && $key eq 'properties') {
                $next_derive_optional = 'properties';
            } elsif ($derive_optional eq 'properties'
                && count_defined_values($value) == 1
                && $value->{optional})
            {
                next;
            }

            if ($value->%*) {
                my $inner_str = '';
                open(my $inner_fh, '>', \$inner_str);
                api_to_string("$indent    ", $inner_fh, $value, $next_derive_optional);
                close($inner_fh);

                if (length($inner_str)) {
                    print {$out} "${indent}$key: {\n";
                    print {$out} $inner_str;
                    print {$out} "$indent},\n";
                }
            }
        } else {
            die "unhandled api value type for '$key': ".ref($value)."\n";
        }
    }
}

my sub print_api_string : prototype($$$) {
    my ($out, $api, $kind) = @_;
    return '' if !$API;

    my $api_str = '';
    open(my $api_str_fh, '>', \$api_str);
    api_to_string("    ", $api_str_fh, $api, $kind);
    close($api_str_fh);
    if (length($api_str)) {
        print {$out} "#[api(\n${api_str})]\n";
    } else {
        print {$out} "#[api]\n";
    }
}

my sub print_struct : prototype($$$) {
    my ($out, $def, $done_array_modules) = @_;

    my @arrays;

    print_api_string($out, $def->{api}, 'struct');

    if (length($def->{description})) {
        print {$out} "$def->{description}\n";
    } elsif ($API) {
        print {$out} "/// Object.\n";
    }
    print_derive($out, $def->{derive});
    print {$out} "pub struct $def->{name} {\n";
    for my $field (sort keys $def->{fields}->%*) {
        my $field_def = $def->{fields}->{$field};
        if ($field_def->{kind} eq 'array-field') {
            push @arrays, $field_def;
        }

        my $name = $field_def->{name};
        my $rust_name = $field_def->{rust_name};
        print {$out} "    $field_def->{description}\n" if length($field_def->{description});
        print {$out} "    $_\n" for $field_def->{attrs}->@*;
        print {$out} "    #[serde(rename = \"$name\")]\n" if $name ne $rust_name;
        print {$out} "    pub $field_def->{rust_name}: $field_def->{type},\n";
        print {$out} "\n";
    }
    print {$out} "}\n";
    for my $array (@arrays) {
        my $module_name = $array->{array_module_name};
        next if $done_array_modules->{$module_name};
        $done_array_modules->{$module_name} = 1;
        my $count = $array->{array_count};
        print {$out} "generate_array_field! {\n";
        print {$out} "    $module_name :\n";
        print {$out} "    $array->{name} :\n";
        print {$out} "    $array->{rust_name}\[${count}] =>\n";
        for my $i (0..($count - 1)) {
            print {$out} "    \"$array->{name}${i}\",\n";
        }
        print {$out} "}\n";
    }
    print {$out} "\n\n";
}

sub print_types : prototype($) {
    my ($out) = @_;

    my $done_array_modules = {};

    print {$out} <<"EOF";
use std::collections::HashMap;

EOF

    if ($API) {
        print {$out} <<"EOF";
use proxmox_schema::api;

EOF
    }

    for my $name (sort keys $all_types->%*) {
        my $def = $all_types->{$name};
        my $kind = $def->{kind};
        if ($kind eq 'struct') {
            print_struct($out, $def, $done_array_modules);
        } elsif ($kind eq 'enum') {
            print_api_string($out, $def->{api}, 'enum');
            print {$out} "$def->{description}\n" if length($def->{description});
            print_derive($out, $def->{derive});
            print {$out} "pub enum $def->{name} {\n";
            for my $variant ($def->{variants}->@*) {
                my ($orig, $named) = @$variant;
                print {$out} "    #[serde(rename = \"$orig\")]\n" if $named ne $orig;
                print {$out} "    /// $orig.\n";
                print {$out} "    $named,\n";
            };
            print {$out} "}\n";
            print {$out} "serde_plain::derive_display_from_serialize!($def->{name});\n";
            print {$out} "serde_plain::derive_fromstr_from_deserialize!($def->{name});\n";
            print {$out} "\n";
        } else {
            die "unhandled kind: $kind\n"
                .'  with properties '.join(', ', sort keys %$def)."\n";
        }
    }
}

my $code_header = <<"CODE";
use proxmox_client::{Error, Environment};

use crate::types::*;
use crate::Client;

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
CODE
my $code_footer = <<"CODE";
}
CODE

my sub print_get_method : prototype($$$) {
    my ($out, $name, $def) = @_;

    my $doc = $def->{description};

    if ($doc && length($doc)) {
        print {$out} to_doc_comment($doc)."\n";
    }
    print {$out} "pub async fn $name(\n";
    print {$out} "    &self,\n";
    my $input = $def->{input};
    my $url_params = $def->{url_params};
    if (defined($input) || $url_params->@*) {
        for my $url_arg ($url_params->@*) {
            my ($arg, $def) = @$url_arg;
            print {$out} "    $arg: $def->{type},\n";
        }
        for my $arg ($input->@*) {
            print {$out} "    $arg->{rust_name}: $arg->{type},\n";
        }
        print {$out} ") -> Result<$def->{output_type}, E::Error> {\n";
        # print {$out} "    // self.login().await?;\n";
        if (@$input) {
            print {$out} "    let (mut query, mut sep) = (String::new(), '?');\n";
            for my $arg (@$input) {
                my $name = $arg->{name};
                my $rust_name = $arg->{rust_name};
                print {$out} "    add_query_arg(&mut query, &mut sep, \"$name\", &$rust_name);\n";
            }
            print {$out} "    let url = format!(\"/api2/extjs$def->{url}\{query}\");\n";
        } else {
            print {$out} "    let url = format!(\"/api2/extjs$def->{url}\");\n";
        }
        if ($def->{'returns-attribs'}) {
            print {$out} "    self.client.get(&url).await\n";
        } else {
            print {$out} <<"EOF";
    Ok(self.client.get(&url).await?
        .data
        .ok_or_else(|| E::Error::bad_api(\"api returned no data\"))?
    )
EOF
        }
    } else {
        die "TODO: GET methods with parameter object (needs to be serialized into query string)\n";
    }

    print {$out} "}\n\n";
}

my sub print_method_with_body : prototype($$$$) {
    my ($out, $name, $def, $method) = @_;

    my $doc = $def->{description};

    if ($doc && length($doc)) {
        print {$out} to_doc_comment($doc)."\n";
    }
    print {$out} "pub async fn $name(\n";
    print {$out} "    &self,\n";
    my $url_params = $def->{url_params};
    for my $url_arg ($url_params->@*) {
        my ($arg, $def) = @$url_arg;
        print {$out} "    $arg: $def->{type},\n";
    }
    print {$out} "    params: $def->{input_type},\n";
    my $output = $def->{output_type} // '()';
    print {$out} ") -> Result<$output, E::Error> {\n";
    # print {$out} "    // self.login().await?;\n";
    print {$out} "    let url = format!(\"/api2/extjs$def->{url}\");\n";
        if ($def->{'returns-attribs'}) {
            print {$out} "    self.${method}_request(&url, &params).await\n";
        } else {
            print {$out} "    Ok(self.${method}_request(&url, &params).await?.data)\n";
        }
    print {$out} "}\n\n";
}

sub print_methods : prototype($) {
    my ($out) = @_;

    print {$out} $code_header;
    for my $name (sort keys $all_methods->%*) {
        my $def = $all_methods->{$name};
        my $http_method = $def->{http_method};
        if ($http_method eq 'GET') {
            print_get_method($out, $name, $def);
        } elsif ($http_method eq 'PUT') {
            print_method_with_body($out, $name, $def, 'put');
        } elsif ($http_method eq 'POST') {
            print_method_with_body($out, $name, $def, 'post');
        } else {
            print "Method $name: ".Dumper($def);
            warn "TODO: $http_method methods\n";
        }
    }
    print {$out} $code_footer;
}

sub register_format : prototype($$) {
    my ($name, $data) = @_;

    $registered_formats->{$name} = $data;
}

sub register_derive : prototype($@) {
    my ($name, @derives) = @_;

    my $list = ($registered_derives->{$name} //= []);
    push @$list, @derives;
}

sub register_enum_variant : prototype($$) {
    my ($from, $to) = @_;
    $rename_enum_variant->{$from} = $to;
}

sub derive : prototype($@) {
    my ($type_name, @types) = @_;
    my $def = $all_types->{$type_name}
        or croak "no such type: $type_name\n";
    $def->{derive}->{$_} = 1 for @types;
}

my %warned = (
    type => 1, # this is handled but not explicitly marked
    # we don't care about these:
    completion => 1,
    renderer => 1,
);
my sub warn_unused : prototype($$) {
    my ($type, $kind) = @_;

    for my $property (sort keys $type->%*) {
        next if $warned{$property};
        $warned{$property} = 1;
        if (defined($kind)) {
            warn "Warning: unhandled type property (for $kind): '$property'\n";
        } else {
            warn "Warning: unhandled type property: '$property'\n";
        }
    }
}

sub namify_field : prototype($) {
    my ($arg) = @_;

    my $out = '';

    my $was_uppercase = 1;
    my $was_underscore = 0;
    for my $i (0..(length($arg)-1)) {
        my $ch = substr($arg, $i, 1);
        $ch = ($ch =~ tr/-/_/r);

        my $is_underscore = ($ch eq '_');
        my $is_uppercase = ($ch =~ /^[A-Z]$/);
        if ($is_uppercase) {
            if (!$was_uppercase && !$was_underscore) {
                $out .= '_';
            }
            $out .= lc($ch);
        } elsif ($ch eq '_') {
            if (!$was_underscore) {
                $out .= $ch;
            }
        } else {
            $out .= lc($ch);
        }
        $was_underscore = $is_underscore;
        $was_uppercase = $is_uppercase;
    }

    return 'ty' if $out eq 'type';

    return $out;
}

my sub namify_type : prototype($;@) {
    my $out = '';
    for my $arg_ (@_) {
        confess "namify_type: undef\n" if !defined $arg_;
        my $arg = ($arg_ =~ s/-/_/gr);

        my $was_underscore = 1;
        my $was_lowercase = 0;
        for my $i (0..(length($arg)-1)) {
            my $ch = substr($arg, $i, 1);
            if ($ch eq '_') {
                $was_underscore = 1;
            } elsif ($was_underscore) {
                $was_underscore = 0;
                $out .= ($ch =~ tr/a-z/A-Z/r);
            } elsif ($was_lowercase) {
                $out .= $ch;
            } else {
                $out .= ($ch =~ tr/A-Z/a-z/r);
            }
            $was_lowercase = ($ch =~ /[^A-Z]/);
        }
    }
    return $out;
}

my sub quote_string : prototype($) {
    my ($s) = @_;
    return '"' . ($s =~ s/(["\\])/\\$1/gr) . '"';
}

my sub indent_lines : prototype($$) {
    my ($indent, $lines) = @_;

    confess "DARN" if !defined($lines);

    my $out = '';

    for my $line (split(/\n/, $lines)) {
        $out .= "\n" if length($out);
        $out .= "${indent}${line}";
    }

    return $out;
}

sub to_doc_comment : prototype($) {
    my ($text) = @_;

    return '' if !defined($text);

    return indent_lines('/// ', $text);
}

my sub type_of : prototype($) {
    my ($schema) = @_;

    my $kind = $schema->{type};
    return $kind if $kind;

    if (exists $schema->{properties}) {
        return 'object';
    } elsif (exists $schema->{items}) {
        return 'array';
    }

    die "missing 'type' and failed to guess\n";
}

my sub get_code_format : prototype($) {
    my ($name) = @_;

    my $info = $registered_formats->{$name}
        or die "info for format '$name' required\n";

    my $code = $info->{code}
        or die "registered format '$name' requires a verify function\n";

    return $code;
}

my sub get_format : prototype($$) {
    my ($format, $name) = @_;

    my $format_name = $name;
    my $format_kind;
    if (!ref($format)) {
        if ($format =~ /^(.*)-a?(list|opt)$/) {
            ($format, $format_kind) = ($1, $2);
            $format_kind = 'array' if $format_kind eq 'list';
        }

        $format_name = $format;
        my $f = PVE::JSONSchema::get_format($format);
        die "missing format '$format'\n" if !$f;
        $format = $f;
    }

    if (ref($format) eq 'HASH') {
        my $name_hint = namify_type($format_name);
        my $type = generate_struct(
            $name_hint,
            { type => 'object', properties => $format, additionalProperties => 0 },
            {},
        );
        return {
            format_name => $format_name,
            kind => $format_kind,
            property_string => $type,
        };
    }

    if (ref($format) eq 'CODE') {
        my $code = get_code_format($format_name);
        return { code => $code };
    }

    die "WEIRD FORMAT TYPE: ".ref($format)."\n";
}

sub integer_type : prototype($$) {
    my ($schema, $api_props) = @_;
    my $min = delete $schema->{minimum};
    my $max = delete $schema->{maximum};

    my $ty;
    if (!defined($min) && !defined($max)) {
        $ty = 'isize';
    } elsif (defined($min) && defined($max)) {
        if ($min >= 0) {
            if ($max < 0x100) {
                $ty = 'u8';
            } elsif ($max < 0x10000) {
                $ty = 'u16';
            } elsif ($max <= 0xffffffff) {
                $ty = 'u32';
            } else {
                $ty = 'u64';
            }
        } elsif ($min >= -0x80 && $max < 0x80) {
            $ty = 'i8';
        } elsif ($min >= -0x8000 && $max < 0x8000) {
            $ty = 'i16';
        } elsif ($min >= -0x80000000 && $max < 0x80000000) {
            $ty = 'i32';
        } else {
            $ty = 'i64';
        }
    } elsif (defined($min)) {
        if ($min >= 0) {
            $ty = 'usize';
        } else {
            $ty = 'isize';
        }
    } else { # defined $max
        $ty = 'isize';
    }

    $api_props->{minimum} = $min if defined $min;
    $api_props->{maximum} = $max if defined $max;

    return $ty;
}

sub number_type : prototype($$) {
    my ($schema, $api_props) = @_;
    my $min = delete $schema->{minimum};
    my $max = delete $schema->{maximum};

    # FIXME: Can we make any good guesses for using `f32`?
    my $ty = 'f64';

    $api_props->{minimum} = $min if defined $min;
    $api_props->{maximum} = $max if defined $max;

    return $ty;
}

my sub check_rust_name : prototype($$) {
    my ($where, $name) = @_;
    if ($name !~ /^[a-zA-Z_][a-zA-Z_0-9]*$/) {
        confess "bad name in $where: $name\n";
    }
}

sub generate_enum : prototype($$) {
    my ($name_hint, $schema) = @_;

    my $dedup_key = join("\0", sort $schema->{enum}->@*);
    if (my $name = $dedup_enum->{$dedup_key}) {
        return $name;
    }
    # If this is not a duplicate we clash, this happens in `/cluster/resources`
    # where the input parameter `type` and the output field `type` get different
    # enum variants.
    $dedup_enum->{$dedup_key} = $name_hint;
    die "duplicate enum name: '$name_hint'\n" if exists $all_enums->{$name_hint};
    #return $name_hint if exists $all_enums->{$name_hint};

    local $__err_path = "$__err_path.$name_hint";
    my $def = {
        kind => 'enum',
        name => $name_hint,
        attrs => [],
        variants => [],
        derive => derive_default(qw(Clone Copy Eq PartialEq)),
        api => {},
        description => '',
    };

    $all_enums->{$name_hint} = $def;
    die "duplicate type name: '$name_hint'\n" if exists $all_types->{$name_hint};
    $all_types->{$name_hint} = $def;

    # Copy so we don't modify the original.
    $schema = { %$schema };

    if (defined(my $description = delete $schema->{description})) {
        $def->{description} = to_doc_comment($description);
    }

    my $variants = $def->{variants};
    my $default = delete $schema->{default};
    my $rust_default;
    for my $variant ((delete $schema->{enum})->@*) {
        my $rust_variant = $rename_enum_variant->{"${name_hint}::$variant"};
        $rust_variant = namify_type($variant) if !defined($rust_variant);
        check_rust_name($name_hint, $rust_variant);
        push $variants->@*, [$variant, $rust_variant];

        if (defined($default) && $default eq $variant) {
            $rust_default = $rust_variant;
        }
    }
    if (defined($default)) {
        if (defined($rust_default)) {
            $def->{default} = $rust_default;
        } else {
            warn "non-existent default enum value '$default'\n" if !defined($rust_default);
        }
    }

    warn_unused($schema, 'enum');

    return $name_hint;
}

my sub string_type : prototype($$$) {
    my ($schema, $api_props, $name_hint) = @_;

    if (defined(my $enum = delete $schema->{enum})) {
        confess "enum string type\n";
    }

    if (defined(my $len = delete $schema->{minLength})) {
        $api_props->{minLength} = $len;
    }
    if (defined(my $len = delete $schema->{maxLength})) {
        $api_props->{maxLength} = $len;
    }
    if (defined(my $text = delete $schema->{typetext})) {
        $api_props->{type_text} = quote_string($text);
    }
    if (defined(my $text = delete $schema->{default})) {
        $api_props->{default} = quote_string($text);
    }

    # pub enum ApiStringFormat {
    #     /// Enumerate all valid strings
    #     Enum(&'static [EnumEntry]),
    #     /// Use a regular expression to describe valid strings.
    #     Pattern(&'static ConstRegexPattern),
    #     /// Use a schema to describe complex types encoded as string.
    #     PropertyString(&'static Schema),
    #     /// Use a verification function.
    #     VerifyFn(ApiStringVerifyFn),
    # }
    if (defined(my $format = delete $schema->{format})) {
        my $fmt = get_format($format, $name_hint);
        if (my $code = $fmt->{code}) {
            $api_props->{format} = "ApiStringFormat::VerifyFn($code)";
        } elsif (my $ty = $fmt->{property_string}) {
            $api_props->{format} = "ApiStringFormat::PropertyString(${ty}::API_SCHEMA)";
        } else {
            confess "FIXME (string_type format stuff)\n" .Dumper($fmt);
        }
        # if (my $kind = $fmt->{kind}) {
        #     $api_props->{format_fixme} = '"LIST TYPE"';
        # }

        # if (my $ps = $fmt->{'property-string'}) {
        #     $api_props->{format} = "ApiStringFormat::PropertyString(${ps}::API_SCHEMA)";
        # }

        # if (my $code = $fmt->{code}) {
        #     $api_props->{format} = "ApiStringFormat::VerifyFn($code)";
        # }
    }

    return 'String';
}

my sub array_type : prototype($$) {
    my ($schema, $name_hint) = @_;

    my $def = {
        kind => 'array',
        name => $name_hint,
        rust_name => namify_type($name_hint),
        type => undef, # rust type
        attrs => [],
        api => {},
        optional => undef,
        description => '',
    };

    my $items = delete $schema->{items} or die "missing 'items' in array schema\n";
    handle_def($def, \$items, $name_hint);

    return "Vec<$def->{type}>";
}

my %serde_int = (
    usize => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_usize")]',
    isize => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]',
    u8 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_u8")]',
    u16 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_u16")]',
    u32 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_u32")]',
    u64 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]',
    i8 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_i8")]',
    i16 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_i16")]',
    i32 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_i32")]',
    i64 => '#[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]',
);

sub handle_def : prototype($$$) {
    my ($def, $inout_schema, $name_hint) = @_;

    my $type = type_of($$inout_schema);
    if ($type eq 'string' && $$inout_schema->{enum}) {
        $def->{type} = generate_enum($name_hint, $$inout_schema);
        return;
    }

    # We need the original schema when generating its type.
    # But we use a copy to deal with its attributes to warn about unused things.
    my $orig_schema = $$inout_schema;
    my $schema = { $orig_schema->%* };
    $$inout_schema = $schema;

    if (defined(my $description = delete $schema->{description})) {
        $def->{description} = to_doc_comment($description);
    }

    if ($type eq 'integer') {
        $def->{type} = integer_type($schema, $def->{api});
        if (defined(my $serde = $serde_int{$def->{type}})) {
            push $def->{attrs}->@*, $serde;
        }
        $def->{api}->{default} = delete $schema->{default};
    } elsif ($type eq 'boolean') {
        $def->{type} = 'bool';
        push $def->{attrs}->@*,
            "#[serde(deserialize_with = \"crate::api_type_helpers::deserialize_bool\")]";
        $def->{api}->{default} = bool(delete $schema->{default});
    } elsif ($type eq 'number') {
        $def->{type} = number_type($schema, $def->{api});
        $def->{api}->{default} = delete $schema->{default};
    } elsif ($type eq 'string') {
        $def->{type} = string_type($schema, $def->{api}, $name_hint);
    } elsif ($type eq 'object') {
        $def->{type} = generate_struct($name_hint, $orig_schema, {});
        # generate_struct uses the original schema and warns by itself
        return;
    } elsif ($type eq 'array') {
        $def->{type} = array_type($schema, $name_hint);
    } else {
        die "unhandled field type: $type\n";
    }

    warn_unused($schema, $type);
}

my sub make_struct_field : prototype($$$$) {
    my ($struct_name, $name, $rust_name, $inout_schema) = @_;
    local $__err_path = "$__err_path.$name";

    my $def = {
        kind => 'field',
        struct => $struct_name,
        name => $name,
        rust_name => $rust_name,
        type => undef, # rust type
        attrs => [],
        api => {},
        optional => undef,
        description => '',
    };


    # in perl the `optional` property is declared in the type's schema but it's not actually part
    # of the type but part of the object-schema so pull it out early to clear the warning:
    my $optional = bool(delete $$inout_schema->{optional});
    handle_def($def, $inout_schema, namify_type($struct_name, $name));

    #my $schema = $$inout_schema;
    # (in case the type was already an option type, don't duplicate the `Option<>`)
    if ($optional && !$def->{optional}) {
        $def->{type} = "Option<$def->{type}>";
        $def->{api}->{optional} = $def->{optional} = true;
        push $def->{attrs}->@*, "#[serde(default, skip_serializing_if = \"Option::is_none\")]";
    }

    return $def;
}

my sub make_struct_array_field : prototype($$$$$) {
    my ($struct_name, $base_name, $base_rust_name, $count, $inout_schema) = @_;
    local $__err_path = "$__err_path.${base_name}[]";

    my $array_module_name = namify_field($struct_name) . '_' . namify_field($base_name);
    my $def = {
        kind => 'array-field',
        struct => $struct_name,
        name => $base_name,
        # FIXME: We cannot just cut off the number because qemu for instance has `numa` and `numaX`
        # but we also don't want to use `numas`...
        rust_name => "${base_rust_name}_array",
        type => undef, # rust type
        attrs => [],
        api => {},
        optional => undef,
        description => '',
        array_module_name => undef,
        array_count => $count,
    };

    # in perl the `optional` property is declared in the type's schema but it's not actually part
    # of the type but part of the object-schema so pull it out early to clear the warning:
    my $optional = bool(delete $$inout_schema->{optional});
    die "array of non-optional values not supported\n" if !$optional;

    handle_def($def, $inout_schema, namify_type($struct_name, $base_name));

    my $array_dedup_key = "$base_name\0$count\0$def->{type}\0";
    if (defined(my $module = $dedup_array_module->{$array_dedup_key})) {
        $array_module_name = $module;
    }
    $dedup_array_module->{$array_dedup_key} = $array_module_name;
    $def->{array_module_name} = $array_module_name;

    $def->{type} = "Box<[Option<$def->{type}>; $count]>";
    $def->{optional} = true;
    push $def->{attrs}->@*, "#[serde(flatten, with = \"$array_module_name\")]";

    return $def;
}

sub is_array : prototype($$) {
    my ($properties, $field) = @_;

    return if $field !~ /^(.*\D)(\d+)$/;

    my ($base, $id) = ($1, $2);
    my $indices = { $id => 1 };
    my $max = $id;

    my $re = qr/^\Q$base\E(\d+)$/;
    for my $key (keys $properties->%*) {
        next if $key !~ $re;
        $indices->{$1} = 1;
        $max = $1 if $1 > $max;
    }

    $indices = [ sort keys $indices->%* ];
    my $count = scalar($indices->@*);
    return if $count <= 1 || ($count != ($max + 1));

    return ($base, $count);
}

sub generate_struct : prototype($$$) {
    my ($name_hint, $schema, $extra) = @_;
    die "no struct name defined, unnamed parameter list?\n" if !defined($name_hint);
    local $__err_path = "$__err_path => struct $name_hint";

    my $properties = $schema->{properties};

    if (!$properties || !$properties->%*) {
        # Special case:
        # {
        #   type => 'object',
        #   properties => {}, # empty
        #   additionalProperties => {SCHEMA},
        # }
        # does not actually become a struct, but a `HashMap<String, TY>`
        my $additional = $schema->{additionalProperties};
        if (ref($additional)) {
            my $name = generate_struct($name_hint, $additional, $extra);
            return "HashMap<String, $name>";
        }
    }

    if (!$properties) {
        my $additional = delete($schema->{additionalProperties}); # default is 1 urrrgh
        if (!$schema->{additionalProperties}) {
            warn "no 'properties' in object schema, using serde_json::Value\n";
            return 'serde_json::Value';
        }
    } else {
        if (my $name = $dedup_struct->{$properties}) {
            return $name;
        }
        $dedup_struct->{$properties} = $name_hint;
    }
    return $name_hint if exists $all_structs->{$name_hint};

    my $def = {
        kind => 'struct',
        name => $name_hint,
        fields => {},
        derive => derive_default(),
        description => '',
        api => {},
        default_key => undef,
    };
    $all_structs->{$name_hint} = $def;
    die "duplicate type name: '$name_hint'\n" if exists $all_types->{$name_hint};
    $all_types->{$name_hint} = $def;

    # Copy so we don't modify the original.
    $schema = { %$schema };
    $properties = delete($schema->{properties}) // {} ;
    # Copy so we don't modify the original.
    $properties = { %$properties };

    # Can't do this here if we use the original for deduping, since the struct may be reused
    # somewhere without URL parameters, too...
    # if (my $skip = $extra->{skip}) {
    #     # delete skipped properties
    #     delete $properties->{$_} for @$skip;
    # }

    if (defined(my $description = delete $schema->{description})) {
        $def->{description} = to_doc_comment($description);
    }

    my @array_bases;
    PROPERTY: for my $field_name (sort keys $properties->%*) {
        next if exists $properties->{$field_name}->{alias};
        for my $base (@array_bases) {
            next PROPERTY if $field_name =~ $base;
        }

        if (my ($base, $count) = is_array($properties, $field_name)) {
            push @array_bases, qr/^\Q$base\E\d+$/;

            my $field_rust_name = namify_field($base);
            my $field_schema = { $properties->{$field_name}->%* };
            $properties->{$field_name} = $field_schema;
            if (delete $field_schema->{default_key}) {
                die "default key points to array element '$field_name'\n";
            }
            my $field =
                make_struct_array_field($name_hint, $base, $field_rust_name, $count, \$field_schema);
            die "duplicate field name '$field_name'\n" if exists($def->{fields}->{$field_name});
            $def->{fields}->{$field_name} = $field;
            $def->{api}->{properties}->{$field_name} = $field->{api};
        } else {
            my $field_rust_name = namify_field($field_name);
            my $field_schema = { $properties->{$field_name}->%* };
            $properties->{$field_name} = $field_schema;
            if (delete $field_schema->{default_key}) {
                $def->{default_key} = $field_name;
            }
            my $field = make_struct_field($name_hint, $field_name, $field_rust_name, \$field_schema);
            die "duplicate field name '$field_name'\n" if exists($def->{fields}->{$field_name});
            $def->{fields}->{$field_name} = $field;
            $def->{api}->{properties}->{$field_name} = $field->{api};
        }
    }

    my $additional = delete($schema->{additionalProperties}); # default is 1 urrrgh
    if (!defined($additional)) {
        # We don't know whether to just ignore it or actually scatter
        # `HashMap<String, serde_json::Value>` entries throughout the code.
        # But if we don't hit this too often the latter is probably fine.
        #
        # Default is 1 so warn...
        # If it's explicit we die (below).
        warn "struct with arbitrary additional properties currently not supported\n";
    } elsif ($additional) {
        if (ref($additional)) {
            # FIXME: proxmox_schema doesn't support this yet.
            die "struct with additional properties not currently supported, don't know how to name the field\n";
        } else {
            die "struct with arbitrary additional properties currently not supported\n"
                if !ref($additional);
        }
    }

    warn_unused($schema, 'struct');

    return $name_hint;
}

my sub get_child_link_info : prototype($) {
    my ($schema) = @_;

    my $links = $schema->{links};
    return if !$links;

    die "unhandled links type in $\n" if @$links != 1;
    $links = $links->[0];
    die "unhandled links 'rel': $links->{rel}\n" if $links->{rel} ne 'child';
    my $href = $links->{href} or die "missing 'href' in 'links'\n";
    die if $href !~ /^\{(.*)\}$/;
    return $1;
}

sub method_return_array : prototype($$$) {
    my ($name_hint, $schema, $extra) = @_;
    die "no array name defined, unnamed return type?\n" if !defined($name_hint);
    local $__err_path = "$__err_path => array $name_hint";

    my $items = $schema->{items} // confess "no 'items' in array schema\n";

    # Special handling of links when the return struct has only a single element:
    if (defined(my $href = get_child_link_info($schema))) {
        my $properties = $items->{properties};
        if ($properties && 1 == keys($properties->%*)) {
            my $property = $properties->{$href}
                or die "link href '$href' does not exist in returned property\n";
            my $type = type_of($property);
            if ($type eq 'string') {
                return 'Vec<String>';
            } else {
                die "unhandled link type '$type' (in href '$href')\n";
            }
        }
    }

    my $def = {
        kind => 'array',
        name => $name_hint,
        items => undef,
        description => '',
        api => {},
        rust_name => $name_hint,
    };

    handle_def($def, \$items, $name_hint);

    return "Vec<$def->{type}>";
}

# Collect all path parameters present in an url.
# (The 'node' and 'vmid' in `/nodes/{node}/qemu/{vmid}/status`.)
my sub url_parameters : prototype($) {
    my ($path) = @_;
    my @params = ($path =~ /\{([^}]+)\}/g);
    return \@params;
}

### Extract method parameters and deal with path based parameters.
# $api_method is the dumped schema's method definition.
my sub method_parameters : prototype($$$$$) {
    my ($def, $api_url, $param_name, $api_method, $rust_method_name) = @_;

    my $url_params = url_parameters($api_url);

    my $parameters = $api_method->{parameters};

    #print "URL PARAMETERS: ".join(', ', $url_params->@*)."\n";

    # Clone to avoid modifying the duplicate.
    $parameters = { $parameters->%* };
    my $properties = { $parameters->{properties}->%* };
    $parameters->{properties} = $properties;

    # Handle URL parameter types. These should only be strings or integers.
    my $url_param_defs = [];
    for my $param ($url_params->@*) {
        my $schema = delete $properties->{$param};

        my $def = {
            kind => 'param',
            name => $param,
            rust_name => namify_field($param),
            type => undef,
            optional => bool(delete($schema->{optional})),
            description => '',
        };

        handle_def($def, \$schema, namify_type($rust_method_name, $param));

        # URL parameters use &str instead of String.
        if ($def->{type} eq 'String') {
            $def->{type} = '&str';
        }

        if ($def->{optional}) {
            $def->{type} = "Option<$def->{type}>";
        }

        push @$url_param_defs, [$param, $def];
    }
    $def->{url_params} = $url_param_defs;


    if ($def->{http_method} eq 'GET' && keys($properties->%*) + $url_params->@* <= 6) {
        # GET methods should only have string parameters, and if there are <= 6 total we won't
        # build a struct.
        my $properties = { $properties->%* };
        my $input = [];
        my $handle = sub {
            my ($param, $schema) = @_;

            my $def = {
                kind => 'param',
                name => $param,
                rust_name => namify_field($param),
                type => undef,
                optional => bool(delete($schema->{optional})),
                description => '',
            };

            handle_def($def, \$schema, namify_type($rust_method_name, $param));
            if ($def->{optional}) {
                $def->{type} = "Option<$def->{type}>";
            }
            push @$input, $def;
        };
        #for my $param ($url_params->@*) {
        #    my $schema = { (delete($properties->{$param}))->%* };
        #    $handle->($param, $schema);
        #}
        for my $param (sort keys $properties->%*) {
            $handle->($param, $properties->{$param});
        }
        $def->{input} = $input;
        return;
    }

    return if !keys($parameters->%*);

    $def->{input_type} = generate_struct(
        $param_name,
        $parameters,
        {}, # { skip => $url_params },
    );
}

### Handle the return type...
#
my sub method_return_type : prototype($$$$) {
    my ($def, $method, $return_name, $extra) = @_;

    if (defined(my $returns = $extra->{'output-type'})) {
        $def->{output_type} = $returns;
        return;
    }

    my $returns = $extra->{'return-type'} // $method->{returns};

    my $type = type_of($returns);
    if ($type eq 'null') {
        return;
    } elsif ($type eq 'object') {
        $def->{output_type} = generate_struct(
            $return_name,
            $returns,
            {},
        );
    } elsif ($type eq 'array') {
        $def->{output_type} = method_return_array(
            $return_name,
            $returns,
            {},
        );
    } elsif ($type eq 'string') {
        my $schema = { $returns->%* };
        my $api = {};
        $def->{output_type} = string_type($schema, $api, $return_name);
        die "unhandled return type api options: ".join(', ', sort keys $api->%*)."\n"
            if $api->%*;
    } else {
        die "unhandled return type: $type\n";
    }

    if ($extra->{attribs}) {
        $def->{output_type} = "ApiResponse<$def->{output_type}>";
        $def->{'returns-attribs'} = 1;
    }
}

### Create an API method.
sub create_method : prototype($$$;%) {
    my (
        $api_url,
        $method,
        $rust_method_name,
        %extra,
    ) = @_;

    die "duplicate method name '$rust_method_name'\n"
        if exists($all_methods->{$rust_method_name});

    local $SIG{__DIE__} = sub {
        die "<$method->{method} ${api_url}>: $__err_path: $_[0]";
    };
    local $SIG{__WARN__} = sub {
        warn "<$method->{method} ${api_url}>: $__err_path: $_[0]";
    };

    my $param_name = $extra{'param-name'};
    my $return_name = $extra{'return-name'};
    if (!defined($return_name) && defined($param_name)) {
        $return_name = "${param_name}Response";
    }

    my $def = {
        kind => 'method',
        url => $api_url,
        name => $rust_method_name,
        http_method => $method->{method},
        description => $method->{description},
    };

    method_parameters($def, $api_url, $param_name, $method, $rust_method_name);
    method_return_type($def, $method, $return_name, \%extra);
    $all_methods->{$rust_method_name} = $def;
}

##################################
#
# Helpers to walk the API method paths

# Maps path nodes in the api tree to their path text.
# (used to show for which methods no wrapper was generated)
my $ALL_PATHS = {};
my $API_ROOT;

# Get a hash of all the unused paths.
sub get_unused_paths() {
    return $ALL_PATHS;
}

my sub walk_api_ : prototype($$$);
sub walk_api_ : prototype($$$) {
    my ($node, $method, $path) = @_;

    if (!length($path)) {
        return (
            $node->{path},
            $node->{info}->{$method} // die "no such method '$method'\n",
        );
    }

    my ($current, $rest) = ($path =~ m!^/*([^/]+)(/.*)?$!);

    for my $entry ($node->{children}->@*) {
        if ($entry->{text} eq $current) {
            return walk_api_($entry, $method, $rest // '');
        }
    }
    die "not found\n";
}

my sub walk_api : prototype($$) {
    my ($method, $path) = @_;
    local $SIG{__DIE__} = sub { die "failed to query path '$path': $_[0]"; };
    return walk_api_({ children => $API_ROOT }, $method, $path);
}

# Create wrappers for an API method.
sub api : prototype($$$;%) {
    my ($method, $api_url, $rust_method_name, %extra) = @_;
    delete $ALL_PATHS->{$api_url};

    (my $path, $method) = walk_api($method, $api_url);
    croak "missing method for '$api_url'\n" if !defined($method);
    create_method($api_url, $method, $rust_method_name, %extra);
}

# Fills $ALL_PATHS recursively from an api node.
my sub collect_paths : prototype($$);
sub collect_paths : prototype($$) {
    my ($node, $path) = @_;
    $ALL_PATHS->{$path} = 1 if length($path);
    my $children = $node->{children};
    return if !$children;

    for my $c ($children->@*) {
        collect_paths($c, "$path/$c->{text}");
    }
}

# Initialize access to an API. Should be a PVE or PMG API root.
sub init_api : prototype($) {
    my ($root) = @_;
    $API_ROOT = $root;
    collect_paths({ children => $API_ROOT }, '');
}

sub debug_path : prototype($$) {
    my ($method, $path) = @_;

    use Data::Dumper;

    $method = walk_api($method, $path);
    print(Dumper($method));
}

1;
