import configparser
import Xlib.display
from subprocess import call


display = Xlib.display.Display()
window = display.get_input_focus().focus
wmclass = window.get_wm_class()

# Options
theme = "default"
i3path = '/home/robin/.config/i3/'
i3themerpath = '/home/robin/.config/i3/themer/'

config = configparser.ConfigParser()

config.read(i3themerpath+'themes/'+theme+'.ini')


def renderi3(window):
    template = open(i3themerpath+'theme.template').read()
    for key in config[window]:
        template = template.replace(key, config[window][key])
    outputConf = open(i3path+'config', 'w')
    outputConf.truncate()
    outputConf.write(template)

def renderi3blocks(window):
    template = open(i3themerpath+'i3blocks.template').read()
    for key in config[window]:
        template = template.replace(key, config[window][key])
    outputConf = open(i3path+'i3blocks.conf', 'w')
    outputConf.truncate()
    outputConf.write(template)


def render(window):
    renderi3(window)
    renderi3blocks(window)
    call(["i3-msg", "reload"])

status = open(i3themerpath+'.status','r')
statstext = status.read()

if wmclass is None:
    if statstext != "desktop":
        render("desktop")
        status = open(i3themerpath+'.status','w')
        status.truncate()
        status.write("desktop")
        print('Set new theme')
else:
    print(wmclass[1])
    if wmclass[1] in config:
        if statstext != wmclass[1]:
            render(wmclass[1])
            status = open(i3themerpath+'.status','w')
            status.truncate()
            status.write(wmclass[1])
            print('Set new theme')
    else:
        if statstext != "default":
            render("default")
            status = open(i3themerpath+'.status','w')
            status.truncate()
            status.write("default")
            print('Set new theme')