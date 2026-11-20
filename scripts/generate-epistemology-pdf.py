#!/usr/bin/env python3
import argparse, html, os, re
from pathlib import Path
from markdown_it import MarkdownIt
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfbase import pdfmetrics
from reportlab.platypus import (
    BaseDocTemplate, PageTemplate, Frame, Paragraph, Spacer, PageBreak,
    Table, TableStyle, Preformatted
)
from reportlab.platypus.tableofcontents import TableOfContents
from reportlab.lib.utils import ImageReader

PAGE_W, PAGE_H = A4
LEFT=18*mm; RIGHT=18*mm; TOP=16*mm; BOTTOM=17*mm
BODY_W=PAGE_W-LEFT-RIGHT

FONT_SANS='Helvetica'; FONT_SANS_B='Helvetica-Bold'; FONT_SERIF_I='Times-Italic'; FONT_MONO='Courier'
for name,path in [
    ('DVSans','/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'),
    ('DVSansB','/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'),
    ('DVSerifI','/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf'),
    ('DVMono','/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf'),
]:
    if os.path.exists(path):
        pdfmetrics.registerFont(TTFont(name,path))
if 'DVSans' in pdfmetrics.getRegisteredFontNames():
    FONT_SANS='DVSans'; FONT_SANS_B='DVSansB'; FONT_SERIF_I='DVSerifI'; FONT_MONO='DVMono'

class H2Paragraph(Paragraph):
    def __init__(self, text, style, key):
        super().__init__(text, style)
        self._toc_text = re.sub('<[^>]+>', '', text)
        self._bookmark_key = key

class NiccoloDoc(BaseDocTemplate):
    def __init__(self, filename, cover_path, **kw):
        super().__init__(filename, pagesize=A4, leftMargin=LEFT, rightMargin=RIGHT, topMargin=TOP, bottomMargin=BOTTOM, **kw)
        self.cover_path=cover_path
        frame=Frame(LEFT,BOTTOM,BODY_W,PAGE_H-TOP-BOTTOM,id='normal')
        self.addPageTemplates([PageTemplate(id='content', frames=[frame], onPage=self._page)])
    def _page(self, canvas, doc):
        p=doc.page
        if p==1:
            canvas.saveState()
            canvas.setFillColor(colors.HexColor('#151515'))
            canvas.rect(0,0,PAGE_W,PAGE_H,fill=1,stroke=0)
            img=ImageReader(self.cover_path); iw,ih=img.getSize()
            scale=min(PAGE_W/iw,PAGE_H/ih)
            w,h=iw*scale,ih*scale; x=(PAGE_W-w)/2; y=(PAGE_H-h)/2
            canvas.drawImage(img,x,y,width=w,height=h,preserveAspectRatio=True,mask='auto')
            canvas.restoreState()
            return
        canvas.saveState()
        canvas.setStrokeColor(colors.HexColor('#cccccc')); canvas.setLineWidth(.35)
        canvas.line(LEFT,PAGE_H-12*mm,PAGE_W-RIGHT,PAGE_H-12*mm)
        canvas.line(LEFT,11*mm,PAGE_W-RIGHT,11*mm)
        canvas.setFont(FONT_SANS_B,6.7); canvas.setFillColor(colors.HexColor('#777777'))
        canvas.drawString(LEFT,PAGE_H-9.7*mm,'NICCOLÒ / AN EPISTEMOLOGY FOR THE REAL WORLD')
        canvas.setFont(FONT_SANS,6.7)
        canvas.drawRightString(PAGE_W-RIGHT,7.1*mm,str(p))
        canvas.restoreState()
    def afterFlowable(self, flowable):
        if isinstance(flowable,H2Paragraph):
            text=flowable._toc_text; key=flowable._bookmark_key
            self.canv.bookmarkPage(key)
            self.canv.addOutlineEntry(text,key,level=0,closed=False)
            self.notify('TOCEntry',(0,text,self.page,key))

styles=getSampleStyleSheet()
S={}
S['body']=ParagraphStyle('body',fontName=FONT_SANS,fontSize=9.2,leading=11.6,spaceAfter=2.4*mm,textColor=colors.HexColor('#151515'))
S['lead']=ParagraphStyle('lead',parent=S['body'],fontSize=10.1,leading=13.0,spaceAfter=3.2*mm)
S['h2']=ParagraphStyle('h2',fontName=FONT_SANS_B,fontSize=16.8,leading=19.0,spaceBefore=5.6*mm,spaceAfter=3.2*mm,textColor=colors.HexColor('#111111'),keepWithNext=True)
S['h3']=ParagraphStyle('h3',fontName=FONT_SANS_B,fontSize=11.4,leading=13.5,spaceBefore=4.0*mm,spaceAfter=2.0*mm,textColor=colors.HexColor('#222222'),keepWithNext=True)
S['h4']=ParagraphStyle('h4',fontName=FONT_SANS_B,fontSize=9.7,leading=11.7,spaceBefore=3.0*mm,spaceAfter=1.5*mm,keepWithNext=True)
S['quote']=ParagraphStyle('quote',fontName=FONT_SERIF_I,fontSize=9.0,leading=12.0,leftIndent=6*mm,rightIndent=4*mm,borderWidth=1,borderColor=colors.HexColor('#777777'),borderPadding=(1.5*mm,0,1.5*mm,4*mm),spaceBefore=1.5*mm,spaceAfter=2.8*mm,textColor=colors.HexColor('#333333'))
S['bullet']=ParagraphStyle('bullet',parent=S['body'],leftIndent=5.5*mm,firstLineIndent=-3.2*mm,bulletIndent=0,spaceAfter=1.1*mm)
S['num']=ParagraphStyle('num',parent=S['body'],leftIndent=6.5*mm,firstLineIndent=-4*mm,spaceAfter=1.1*mm)
S['code']=ParagraphStyle('code',fontName=FONT_MONO,fontSize=7.2,leading=9.0,leftIndent=4*mm,rightIndent=2*mm,borderWidth=.5,borderColor=colors.HexColor('#dddddd'),borderPadding=2.2*mm,backColor=colors.HexColor('#f5f5f5'),spaceBefore=1.5*mm,spaceAfter=2.5*mm)
S['toc_title']=ParagraphStyle('toc_title',fontName=FONT_SANS_B,fontSize=21,leading=24,spaceAfter=6*mm)

def inline_html(children):
    out=[]
    for t in children or []:
        ty=t.type
        if ty=='text': out.append(html.escape(t.content))
        elif ty=='softbreak': out.append(' ')
        elif ty=='hardbreak': out.append('<br/>')
        elif ty=='strong_open': out.append('<b>')
        elif ty=='strong_close': out.append('</b>')
        elif ty=='em_open': out.append('<i>')
        elif ty=='em_close': out.append('</i>')
        elif ty=='code_inline': out.append('<font name="%s">%s</font>'%(FONT_MONO,html.escape(t.content)))
        elif ty=='link_open':
            href=t.attrGet('href') or ''
            out.append('<a href="%s" color="#333333">'%html.escape(href,quote=True))
        elif ty=='link_close': out.append('</a>')
        elif t.content: out.append(html.escape(t.content))
    return ''.join(out)

def parse_md(md_text):
    md=MarkdownIt('commonmark',{'html':False,'linkify':False,'typographer':False}).enable('table')
    toks=md.parse(md_text)
    story=[PageBreak()]
    toc=TableOfContents()
    toc.levelStyles=[ParagraphStyle('TOC0',fontName=FONT_SANS_B,fontSize=8.4,leading=10.6,leftIndent=2*mm,spaceAfter=.6*mm)]
    story.extend([Paragraph('CONTENTS',S['toc_title']),toc,PageBreak()])
    i=0; h2_count=0; first_para=True; list_stack=[]
    while i<len(toks):
        t=toks[i]
        if t.type=='heading_open':
            lvl=int(t.tag[1]); content=inline_html(toks[i+1].children)
            if lvl==1:
                i+=3; continue
            if lvl==2:
                h2_count+=1; story.append(H2Paragraph(content,S['h2'],f'h2_{h2_count:03d}'))
            elif lvl==3: story.append(Paragraph(content,S['h3']))
            else: story.append(Paragraph(content,S['h4']))
            i+=3; continue
        if t.type=='paragraph_open':
            content=inline_html(toks[i+1].children); st=S['lead'] if first_para else S['body']; first_para=False
            if list_stack:
                marker=list_stack[-1]
                prefix='•' if marker[0]=='bullet' else f'{marker[1]}. '
                story.append(Paragraph(f'{prefix} {content}',S['bullet'] if marker[0]=='bullet' else S['num']))
                if marker[0]=='ordered': marker[1]+=1
            else:
                story.append(Paragraph(content,st))
            i+=3; continue
        if t.type=='blockquote_open':
            parts=[]; i+=1
            while i<len(toks) and toks[i].type!='blockquote_close':
                if toks[i].type=='inline': parts.append(inline_html(toks[i].children))
                i+=1
            story.append(Paragraph('<br/>'.join(parts),S['quote'])); i+=1; continue
        if t.type in ('fence','code_block'):
            story.append(Preformatted(t.content.rstrip('\n'),S['code'],maxLineLength=120)); i+=1; continue
        if t.type=='bullet_list_open': list_stack.append(['bullet',1]); i+=1; continue
        if t.type=='ordered_list_open': list_stack.append(['ordered',int(t.attrGet('start') or 1)]); i+=1; continue
        if t.type in ('bullet_list_close','ordered_list_close'):
            list_stack.pop(); story.append(Spacer(1,1.2*mm)); i+=1; continue
        if t.type=='table_open':
            rows=[]; header_rows=0; i+=1; current=[]
            while i<len(toks) and toks[i].type!='table_close':
                tt=toks[i]
                if tt.type=='thead_open': header_rows=1
                if tt.type=='tr_open': current=[]
                elif tt.type=='inline':
                    current.append(Paragraph(inline_html(tt.children),ParagraphStyle('cell',parent=S['body'],fontSize=7.0,leading=8.4,spaceAfter=0)))
                elif tt.type=='tr_close': rows.append(current)
                i+=1
            if rows:
                n=max(len(r) for r in rows)
                table=Table(rows,colWidths=[BODY_W/n]*n,repeatRows=header_rows,hAlign='LEFT')
                ts=[('VALIGN',(0,0),(-1,-1),'TOP'),('GRID',(0,0),(-1,-1),.3,colors.HexColor('#dddddd')),('LEFTPADDING',(0,0),(-1,-1),2*mm),('RIGHTPADDING',(0,0),(-1,-1),2*mm),('TOPPADDING',(0,0),(-1,-1),1.4*mm),('BOTTOMPADDING',(0,0),(-1,-1),1.4*mm)]
                if header_rows: ts += [('BACKGROUND',(0,0),(-1,0),colors.HexColor('#eeeeee')),('FONTNAME',(0,0),(-1,0),FONT_SANS_B)]
                table.setStyle(TableStyle(ts)); story.append(table); story.append(Spacer(1,2*mm))
            i+=1; continue
        if t.type=='hr': story.append(Spacer(1,2.5*mm)); i+=1; continue
        i+=1
    return story

def build(md_path, cover_path, out_path):
    md=Path(md_path).read_text(encoding='utf-8')
    doc=NiccoloDoc(out_path,cover_path,title='Niccolò: An Epistemology for the Real World',subject='Product and conceptual architecture overview',author='')
    doc.multiBuild(parse_md(md))
    from pypdf import PdfReader, PdfWriter
    r=PdfReader(out_path); w=PdfWriter(); w.clone_document_from_reader(r)
    info=w._info.get_object(); info.clear()
    w.add_metadata({'/Title':'Niccolò: An Epistemology for the Real World','/Subject':'Product and conceptual architecture overview','/Author':''})
    tmp=out_path+'.normalized'
    with open(tmp,'wb') as f: w.write(f)
    os.replace(tmp,out_path)

if __name__=='__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('md'); ap.add_argument('cover'); ap.add_argument('out')
    a=ap.parse_args(); build(a.md,a.cover,a.out)
