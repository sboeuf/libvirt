#!/usr/bin/perl
#
# bracket-spacing.pl: Report any usage of 'function (..args..)'
# Also check for other syntax issues, such as correct use of ';'
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
#
# This library is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
# Lesser General Public License for more details.
#
# You should have received a copy of the GNU Lesser General Public
# License along with this library.  If not, see
# <http://www.gnu.org/licenses/>.
#
# Authors:
#     Daniel P. Berrange <berrange@redhat.com>

use strict;
use warnings;

my $ret = 0;
my $incomment = 0;

foreach my $file (@ARGV) {
    open FILE, $file;

    while (defined (my $line = <FILE>)) {
        my $data = $line;
        # For temporary modifications
        my $tmpdata;

        # Kill any quoted , ; = or "
        $data =~ s/'[";,=]'/'X'/g;

        # Kill any quoted strings
        $data =~ s,"([^\\\"]|\\.)*","XXX",g;

        # Kill any C++ style comments
        $data =~ s,//.*$,//,;

        next if $data =~ /^#/;

        # Kill contents of multi-line comments
        # and detect end of multi-line comments
        if ($incomment) {
            if ($data =~ m,\*/,) {
                $incomment = 0;
                $data =~ s,^.*\*/,*/,;
            } else {
                $data = "";
            }
        }

        # Kill single line comments, and detect
        # start of multi-line comments
        if ($data =~ m,/\*.*\*/,) {
            $data =~ s,/\*.*\*/,/* */,;
        } elsif ($data =~ m,/\*,) {
            $incomment = 1;
            $data =~ s,/\*.*,/*,;
        }

        # We need to match things like
        #
        #  int foo (int bar, bool wizz);
        #  foo (bar, wizz);
        #
        # but not match things like:
        #
        #  typedef int (*foo)(bar wizz)
        #
        # we can't do this (efficiently) without
        # missing things like
        #
        #  foo (*bar, wizz);
        #
        # We also don't want to spoil the $data so it can be used
        # later on.
        $tmpdata = $data;
        while ($tmpdata =~ /(\w+)\s\((?!\*)/) {
            my $kw = $1;

            # Allow space after keywords only
            if ($kw =~ /^(if|for|while|switch|return)$/) {
                $tmpdata =~ s/($kw\s\()/XXX(/;
            } else {
                print "$file:$.: $line";
                $ret = 1;
                last;
            }
        }

        # Require whitespace immediately after keywords,
        # but none after the opening bracket
        if ($data =~ /\b(if|for|while|switch|return)\(/ ||
            $data =~ /\b(if|for|while|switch|return)\s+\(\s/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Forbid whitespace between )( of a function typedef
        if ($data =~ /\(\*\w+\)\s+\(/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Forbid whitespace following ( or prior to )
        if ($data =~ /\S\s+\)/ ||
            $data =~ /\(\s+\S/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Forbid whitespace before ";" or ",". Things like below are allowed:
        #
        # 1) The expression is empty for "for" loop. E.g.
        #   for (i = 0; ; i++)
        #
        # 2) An empty statement. E.g.
        #   while (write(statuswrite, &status, 1) == -1 &&
        #          errno == EINTR)
        #       ;
        #
        if ($data =~ /[^;\s]\s+[;,]/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Require EOL, macro line continuation, or whitespace after ";".
        # Allow "for (;;)" as an exception.
        if ($data =~ /;[^	 \\\n;)]/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Require EOL, space, or enum/struct end after comma.
        if ($data =~ /,[^ \\\n)}]/) {
            print "$file:$.: $line";
            $ret = 1;
        }

        # Require spaces around assignment '=', compounds and '=='
        # with the exception of virAssertCmpInt()
        $tmpdata = $data;
        $tmpdata =~ s/(virAssertCmpInt\(.* ).?=,/$1op,/;
        if ($tmpdata =~ /[^ ]\b[!<>&|\-+*\/%\^=]?=[^=]/ ||
            $tmpdata =~ /=[^= \\\n]/) {
            print "$file:$.: $line";
            $ret = 1;
        }
    }
    close FILE;
}

exit $ret;
