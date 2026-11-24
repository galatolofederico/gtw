# fit.py in.png out.png [margin]: crop a --still shot (stills.sh) to its alpha bounds and fit it to 680 x 392.
import sys
from PIL import Image
im=Image.open(sys.argv[1]).convert('RGBA'); m=float(sys.argv[3]) if len(sys.argv)>3 else 0.04
bb=im.getchannel('A').point(lambda a:255 if a>8 else 0).getbbox(); im=im.crop(bb)
W,H=680,392; k=min(W*(1-2*m)/im.width, H*(1-2*m)/im.height)
im=im.resize((round(im.width*k),round(im.height*k)),Image.LANCZOS)
o=Image.new('RGBA',(W,H),(0,0,0,0)); o.alpha_composite(im,((W-im.width)//2,(H-im.height)//2)); o.save(sys.argv[2],optimize=True)
