#!/usr/bin/env python3

# Modified version of https://github.com/KhronosGroup/OpenCL-ICD-Loader/blob/main/scripts/gen/__init__.py.
#
# Run using:
# ./gen-shim.py -registry "https://raw.githubusercontent.com/KhronosGroup/OpenCL-Docs/main/xml/cl.xml"

from collections import OrderedDict
from collections import namedtuple

import argparse
import sys
import urllib
import xml.etree.ElementTree as etree
import urllib.request

# parse_xml - Helper function to parse the XML file from a URL or local file.
def parse_xml(path):
    file = urllib.request.urlopen(path) if path.startswith("http") else open(path, 'r')
    with file:
        tree = etree.parse(file)
        return tree

# noneStr - returns string argument, or "" if argument is None.
def noneStr(s):
    if s:
        return s
    return ""

def parse_args():
    parser = argparse.ArgumentParser()

    # To pull the latest registry file from GitHub, pass:
    # -registry "https://raw.githubusercontent.com/KhronosGroup/OpenCL-Docs/main/xml/cl.xml"

    parser.add_argument('-registry', action='store',
                        default='cl.xml',
                        help='Use specified registry file instead of cl.xml')
    parser.add_argument('-o', action='store', dest='directory',
                        default='.',
                        help='Create target and related files in specified directory')

    args = parser.parse_args()
    return args

def load_spec(args):
    specpath = args.registry

    print('Parsing XML file from: ' + specpath)
    spec = parse_xml(specpath)
    return spec

def get_apisigs(spec):
    # Generate the API function signatures dictionary:
    apisigs = OrderedDict()
    ApiSignature = namedtuple('ApiSignature', 'Name RetType Params Suffix')
    ApiParam = namedtuple('ApiParam', 'Type TypeEnd Name')
    print('Generating API signatures dictionary...')
    for command in spec.findall('commands/command'):
        suffix = noneStr(command.get('suffix'))
        proto = command.find('proto')
        ret = noneStr(proto.text)
        name = ""
        params = ""
        for elem in proto:
            if elem.tag == 'name':
                name = noneStr(elem.text) + noneStr(elem.tail)
            else:
                ret = ret + noneStr(elem.text) + noneStr(elem.tail)
        ret = ret.strip()
        name = name.strip()

        plist = []
        for param in command.findall('param'):
            ptype = noneStr(param.text)
            ptypeend = ""
            pname = ""
            for elem in param:
                if elem.tag == 'name':
                    pname = noneStr(elem.text)
                    ptypeend = noneStr(elem.tail)
                else:
                    ptype = ptype + noneStr(elem.text) + noneStr(elem.tail)
            ptype = ptype.strip()
            ptypeend = ptypeend.strip()
            pname = pname.strip()
            plist.append(ApiParam(ptype, ptypeend, pname))

        # For an empty parameter list (for e.g. clUnloadCompiler), add a single
        # unnamed void parameter to make generation easier.
        if len(plist) == 0:
            plist.append(ApiParam("void", "", ""))

        apisigs[name] = ApiSignature(name, ret, plist, suffix)
    return apisigs

def get_apis(spec, apisigs):
    # Generate the core API dictionary:
    coreapis = OrderedDict()
    print('Generating core API dictionary...')
    for feature in spec.findall('feature'):
        version = noneStr(feature.get('name'))

        alist = []
        for function in feature.findall('require/command'):
            name = function.get('name')
            alist.append(apisigs[name])
        coreapis[version] = alist

    # Generate the extensions API dictionary:
    extapis = OrderedDict()
    print('Generating API extensions dictionary...')
    for feature in spec.findall('extensions/extension'):
        extension = noneStr(feature.get('name'))

        alist = []
        for function in feature.findall('require/command'):
            name = function.get('name')
            alist.append(apisigs[name])
        extapis[extension] = alist
    return (coreapis, extapis)



# Modifications below:
import os

if __name__ == "__main__":
    dir_path = os.path.dirname(os.path.realpath(__file__))
    args = parse_args()
    spec = load_spec(args)
    apisigs = get_apisigs(spec)
    (coreapis, extapis) = get_apis(spec, apisigs)

    with open(os.path.join(dir_path, 'opencl-functions.inc'), 'w') as f:
        f.write("// auto-generated with gen-shim.py\n")
        f.write("// X(required, return-kind, return-type, name, declaration, arguments)\n")

        for version in coreapis:
            print(version)
            # Required OpenCL 1.2 API plus optional newer calls.
            is_required = version in ["CL_VERSION_1_0", "CL_VERSION_1_1", "CL_VERSION_1_2"]
            for (name, ret_type, params, suffix) in coreapis[version]:
                if name == "clSetCommandQueueProperty":
                    continue # Skip this API, it's been removed from > 1.0

                declaration = ", ".join(f"{type} {name}{type_end}" for (type, type_end, name) in params)
                arguments = ", ".join(name for (type, type_end, name) in params)
                if name == "clGetPlatformIDs":
                    return_kind = "PLATFORM"
                elif ret_type == "cl_int":
                    return_kind = "INT"
                elif ret_type == "void*":
                    return_kind = "PTR"
                elif name == "clSVMFree":
                    return_kind = "NONE"
                else:
                    return_kind = "HANDLE"
                f.write(f"X({1 if is_required else 0}, {return_kind}, {ret_type}, {name}, ({declaration}), ({arguments}))\n")

        for name in extapis:
            print(f"skipped extension API `{name}`")
